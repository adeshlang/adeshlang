//! JIT Backend for AdeshLang
//!
//! This module provides a JIT compiler that transforms VIR (or LIR) into native
//! machine code. Supports both VIR (Value IR) and LIR (Low-level IR) input via
//! a bridge adapter. VIR is the preferred input format.
//!
//! Includes recursion optimizations:
//! - Memoization for pure recursive functions
//! - Tail call optimization (TCO)
//! - Trampolining for deep recursion

mod cranelift_impl;

use super::unsafe_heap;
use crate::utils::collections::FastMap;
use num_bigint::BigInt;
use std::sync::Arc;

use super::builtins::{BuiltinRegistry, CallableFunction, PROMISE_RUNTIME, RuntimeValue};
use super::lir::{BlockId, LirFunction, LirInst, LirModule};
use super::recursion_opt::{RecursionOptConfig, RecursionOptimizer};

// VIR support
use crate::backends::common::vir_lir_bridge::VirToLirBridge;
use crate::ir::vir::VirModule;

use cranelift_impl::{JitFrame, runtime_type_name, sizeof_runtime_value};
pub use cranelift_impl::{JitMemoryStats, JitVariableInfo};

pub use cranelift_impl::api::{jit_run, jit_run_with_stats, jit_run_with_stats_and_config};

/// Cached compiled module with Arc for efficient sharing
#[derive(Clone)]
pub struct CachedModule {
    /// The compiled LIR module
    pub lir: Arc<LirModule>,
    /// Namespace object containing module exports
    pub namespace: Arc<FastMap<String, RuntimeValue>>,
}

/// JIT execution context with recursion optimizations and module loading
pub struct JitContext {
    builtins: BuiltinRegistry,
    globals: FastMap<String, RuntimeValue>,
    functions: FastMap<String, LirFunction>,
    /// Recursion optimizer for TCO, memoization, etc.
    recursion_opt: RecursionOptimizer,
    /// Current recursion depth for stack overflow protection
    recursion_depth: u32,
    /// Maximum allowed recursion depth (dynamically calculated)
    max_recursion: u32,
    /// Adaptive memory configuration
    #[allow(dead_code)]
    adaptive_memory: crate::memory::adaptive::AdaptiveMemoryConfig,
    /// Module cache: path -> compiled module with exports (Arc for zero-copy sharing)
    module_cache: FastMap<String, CachedModule>,
    /// Import aliases: alias -> module path
    import_aliases: FastMap<String, String>,
    /// ARC-managed values: ArcId -> RuntimeValue
    arc_values: FastMap<u64, Arc<std::sync::Mutex<RuntimeValue>>>,
    /// ARC reference counts: ArcId -> strong_count
    arc_strong_counts: FastMap<u64, Arc<std::sync::atomic::AtomicUsize>>,
    /// Weak reference counts: ArcId -> weak_count
    arc_weak_counts: FastMap<u64, Arc<std::sync::atomic::AtomicUsize>>,
    /// Next ARC ID
    next_arc_id: u64,
    /// Cache for resolved extended methods to avoid repeated lookups.
    extended_method_cache: FastMap<(String, String), LirFunction>,
}

impl JitContext {
    /// Create a new JIT context with default recursion optimizations
    pub fn new() -> Self {
        let adaptive_memory = crate::memory::adaptive::AdaptiveMemoryConfig::default();
        let max_recursion =
            adaptive_memory.max_recursion_depth(adaptive_memory.initial_stack_size) as u32;

        let mut ctx = JitContext {
            builtins: BuiltinRegistry::new(),
            globals: FastMap::default(),
            functions: FastMap::default(),
            recursion_opt: RecursionOptimizer::default(),
            recursion_depth: 0,
            max_recursion,
            adaptive_memory,
            module_cache: FastMap::default(),
            import_aliases: FastMap::default(),
            arc_values: FastMap::default(),
            arc_strong_counts: FastMap::default(),
            arc_weak_counts: FastMap::default(),
            next_arc_id: 1,
            extended_method_cache: FastMap::default(),
        };

        // Initialize the input object with methods as a special builtin namespace
        ctx.setup_input_namespace();

        ctx
    }

    /// Create a JIT context with custom recursion optimization config
    pub fn with_recursion_opt(config: RecursionOptConfig) -> Self {
        let adaptive_memory = crate::memory::adaptive::AdaptiveMemoryConfig::default();
        let max_recursion =
            adaptive_memory.max_recursion_depth(adaptive_memory.initial_stack_size) as u32;

        let mut ctx = JitContext {
            builtins: BuiltinRegistry::new(),
            globals: FastMap::default(),
            functions: FastMap::default(),
            recursion_opt: RecursionOptimizer::new(config),
            recursion_depth: 0,
            max_recursion,
            adaptive_memory,
            module_cache: FastMap::default(),
            import_aliases: FastMap::default(),
            arc_values: FastMap::default(),
            arc_strong_counts: FastMap::default(),
            arc_weak_counts: FastMap::default(),
            next_arc_id: 1,
            extended_method_cache: FastMap::default(),
        };

        ctx.setup_input_namespace();

        ctx
    }

    /// Setup the input namespace object with methods like mock, select, form, etc.
    /// This allows input.mock(), input.select(), etc. to work in JIT

    /// Load a VIR module into the JIT context (converts to LIR via bridge)
    pub fn load_vir_module(&mut self, vir_module: &VirModule) -> Result<(), String> {
        // Convert VIR to LIR using the bridge
        let mut bridge = VirToLirBridge::new();
        let lir_module = bridge.convert_module(vir_module)?;

        // Load the converted LIR module
        self.load_module(&lir_module);
        Ok(())
    }

    /// Compile a VIR module and return a cached module (for module cache)
    pub fn compile_vir_module(&mut self, vir_module: &VirModule) -> Result<CachedModule, String> {
        // Convert VIR to LIR using the bridge
        let mut bridge = VirToLirBridge::new();
        let lir_module = bridge.convert_module(vir_module)?;

        // Load the module to populate functions and prepare for execution
        self.load_module(&lir_module);

        // Create cached module with the LIR and an empty namespace
        // The namespace will be populated when the module is executed
        Ok(CachedModule {
            lir: Arc::new(lir_module),
            namespace: Arc::new(FastMap::default()),
        })
    }

    /// Load an LIR module into the JIT context
    pub fn load_module(&mut self, module: &LirModule) {
        // Store import aliases from the module
        for (alias, path) in &module.imports {
            self.import_aliases.insert(alias.clone(), path.clone());
        }

        // Prepare recursion optimizations (analysis)
        self.recursion_opt.prepare_module(module);

        // Load functions, applying TCO where applicable
        for func in &module.functions {
            // Check if this function is eligible for TCO
            if let Some(analysis) = self.recursion_opt.analyzer().get_analysis(&func.name) {
                if let Some(optimized) = self.recursion_opt.tco().optimize(func, analysis) {
                    // Insert the TCO-optimized version
                    self.functions.insert(func.name.clone(), optimized);
                    continue;
                }
            }
            // Insert the original function
            self.functions.insert(func.name.clone(), func.clone());
        }
    }

    /// Collect memory statistics about the current JIT context
    pub fn get_memory_stats(&self) -> JitMemoryStats {
        let mut stats = JitMemoryStats::default();
        let mut by_type: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();

        // Count functions (excluding 'main' wrapper)
        stats.function_count = self.functions.len().saturating_sub(1);

        // Promise count - we can't easily access this from here, set to 0
        stats.promise_count = 0;

        // Memoization cache - approximate based on recursion opt state
        stats.memo_cache_entries = 0;

        // Iterate through globals to collect variable info
        for (name, value) in &self.globals {
            // Skip internal/builtin names
            if name.starts_with("__") || name.starts_with("_") {
                continue;
            }

            let size = sizeof_runtime_value(value);
            let type_name = runtime_type_name(value).to_string();

            // Update by-type stats
            let entry = by_type.entry(type_name.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += size;

            stats.variables.push(JitVariableInfo {
                name: name.clone(),
                type_name,
                size_bytes: size,
            });

            stats.total_variable_bytes += size;
            stats.variable_count += 1;
        }

        stats.by_type = by_type;

        // Sort variables by size (largest first)
        stats
            .variables
            .sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

        stats
    }

    /// Execute the main function
    pub fn run_main(&mut self) -> Result<RuntimeValue, String> {
        // Check if both __user_main (main) and __top_level_wrapper exist
        let has_wrapper = self.functions.contains_key("__top_level_wrapper");
        let has_main = self.functions.contains_key("main");

        // Prefer the generated entry `main` which handles both wrapper + user main
        let result = if let Some(entry_main) = self.functions.get("main").cloned() {
            self.execute_function(&entry_main, vec![])?
        } else if has_wrapper {
            // Fallback: run wrapper only
            let wrapper = self.functions.get("__top_level_wrapper").cloned().unwrap();
            self.execute_function(&wrapper, vec![])?
        } else if has_main {
            // Fallback: run user main directly
            let user_main = self.functions.get("__user_main").cloned().unwrap();
            self.execute_function(&user_main, vec![])?
        } else {
            RuntimeValue::Null
        };

        // Process any remaining microtasks (Promise callbacks, timers, etc.)
        // This ensures that all scheduled async work completes before the program exits
        // We need to wait for both microtasks AND pending timer threads
        let max_drain_iterations = 100000;
        let mut drain_count = 0;
        let mut idle_count = 0;

        while drain_count < max_drain_iterations {
            // Process any queued microtasks
            if PROMISE_RUNTIME.has_microtasks() {
                self.process_microtasks()?;
                idle_count = 0;
                drain_count += 1;
                continue;
            }

            // Check if there are pending timers
            if PROMISE_RUNTIME.has_pending_timers() {
                // Small sleep to allow timer threads to complete and queue their microtasks
                std::thread::sleep(std::time::Duration::from_millis(1));
                idle_count += 1;
                drain_count += 1;
                // Safety limit - don't wait forever for timers (reduced from 5s to 100ms)
                if idle_count > 100 {
                    // ~100ms max wait for timers
                    // Swallow pending timer warnings for JIT to reduce debug spam
                    break;
                }
                continue;
            }

            // No microtasks and no pending timers - we're done
            break;
        }

        if let Some(exc) = self.check_and_clear_exception() {
            let msg = match &exc {
                RuntimeValue::Object(obj) => {
                    if let Some(RuntimeValue::String(s)) = obj.get("message") {
                        s.clone()
                    } else {
                        exc.as_string()
                    }
                }
                _ => exc.as_string(),
            };
            Err(format!("Unhandled exception: {}", msg))
        } else {
            Ok(result)
        }
    }

    /// Execute a named function with arguments
    pub fn run_function(
        &mut self,
        name: &str,
        args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        if let Some(func) = self.functions.get(name).cloned() {
            self.execute_function(&func, args)
        } else {
            Err(format!("Function '{}' not found", name))
        }
    }

    /// Execute a function with memoization support
    fn execute_function(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        // Check recursion depth
        self.recursion_depth += 1;
        if self.recursion_depth > self.max_recursion {
            self.recursion_depth -= 1;
            return Err(format!(
                "Maximum recursion depth ({}) exceeded",
                self.max_recursion
            ));
        }

        // Try memoization cache first
        let func_name = func.name.clone();
        if self.recursion_opt.memo().is_memoized(&func_name) {
            // For single numeric argument, use optimized int cache
            if args.len() == 1 {
                if let Some(n) = args[0].as_int() {
                    if let Some(cached) = self.recursion_opt.memo().get_cached_int(&func_name, n) {
                        self.recursion_depth -= 1;
                        return Ok(cached);
                    }
                }
            }
            // For multiple arguments, use multi-arg cache
            if let Some(cached) = self
                .recursion_opt
                .memo()
                .get_cached_multi(&func_name, &args)
            {
                self.recursion_depth -= 1;
                return Ok(cached);
            }
        }

        // Execute the function
        let result = self.execute_function_impl(func, args.clone())?;

        // Cache result if memoized
        if self.recursion_opt.memo().is_memoized(&func_name) {
            if args.len() == 1 {
                if let Some(n) = args[0].as_int() {
                    self.recursion_opt
                        .memo()
                        .cache_int(&func_name, n, result.clone());
                }
            } else {
                self.recursion_opt
                    .memo()
                    .cache_multi(&func_name, &args, result.clone());
            }
        }

        self.recursion_depth -= 1;
        Ok(result)
    }

    /// Execute function with captured variables (for closures)
    fn execute_function_with_captures(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
        captures: &FastMap<String, RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        // Check recursion depth
        self.recursion_depth += 1;
        if self.recursion_depth > self.max_recursion {
            self.recursion_depth -= 1;
            return Err(format!(
                "Maximum recursion depth ({}) exceeded",
                self.max_recursion
            ));
        }

        let mut frame = JitFrame::new();

        // Bind captured variables as __capture_{name} to match how lambda functions expect them
        for (name, value) in captures {
            frame.set_var(format!("__capture_{}", name), value.clone());
        }

        // Bind arguments to parameters (skip the hidden __capture_ params)
        let regular_params: Vec<_> = func
            .params
            .iter()
            .filter(|(name, _)| !name.starts_with("__capture_"))
            .collect();
        for (i, (name, _ty)) in regular_params.iter().enumerate() {
            let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
            frame.set_var(name.clone(), value);
        }

        // Start executing from the entry block
        let mut current_block = func.entry_block; // Reasonable limit for complex programs

        loop {
            let block = func
                .get_block(current_block)
                .ok_or_else(|| format!("Block {} not found", current_block))?;

            let mut jumped = false;
            for inst in &block.instructions {
                match self.execute_instruction(&mut frame, inst, func)? {
                    ControlFlow::Next => {}
                    ControlFlow::Jump(target) => {
                        current_block = target;
                        jumped = true;
                        break;
                    }
                    ControlFlow::Return(value) => {
                        self.recursion_depth -= 1;
                        return Ok(value);
                    }
                    ControlFlow::TailCall(name, new_args) => {
                        // TCO: Instead of recursing, update params and restart
                        if name == func.name {
                            // Self tail-call - restart with new args (no stack growth!)
                            let regular_params: Vec<_> = func
                                .params
                                .iter()
                                .filter(|(pname, _)| !pname.starts_with("__capture_"))
                                .collect();
                            for (i, (pname, _ty)) in regular_params.iter().enumerate() {
                                let value = new_args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                                frame.set_var(pname.clone(), value);
                            }
                            current_block = func.entry_block;
                            jumped = true;
                            break;
                        } else {
                            // Tail call to different function - still need to call it
                            self.recursion_depth -= 1;
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                return self.execute_function(&target_func, new_args);
                            }
                            return Ok(RuntimeValue::Null);
                        }
                    }
                }
            }

            // If we didn't jump and finished all instructions in block, we need to exit
            // This prevents infinite loops when a block has no explicit return/jump
            if !jumped {
                self.recursion_depth -= 1;
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Call an async function - returns a Promise immediately and executes the function body
    /// Internal function execution (with TCO support)
    fn execute_function_impl(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        let mut current_args = args;

        // TCO outer loop - allows restarting with new arguments
        'tco: loop {
            // Bind arguments to parameters - create fresh frame each iteration for TCO
            let mut frame = JitFrame::new();
            for (i, (name, _ty)) in func.params.iter().enumerate() {
                let value = current_args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value.clone());
                // Also seed the value slot if var_map maps this parameter to a ValueId
                // This is needed for VIR-generated LIR which uses ValueId-based references
                if let Some(value_id) = func.get_var(name) {
                    frame.set_value(value_id, value);
                }
            }

            // Start executing from the entry block
            let mut current_block = func.entry_block; // Reasonable limit to prevent infinite loops

            loop {
                let block = func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                let mut jumped = false;
                for inst in &block.instructions {
                    match self.execute_instruction(&mut frame, inst, func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            jumped = true;
                            break;
                        }
                        ControlFlow::Return(value) => {
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            // TCO: Instead of recursing, update params and restart
                            if name == func.name {
                                // Self tail-call - restart with new args (no stack growth!)
                                current_args = new_args;
                                continue 'tco;
                            } else {
                                // Tail call to different function - still need to call it
                                if let Some(target_func) = self.functions.get(&name).cloned() {
                                    return self.execute_function(&target_func, new_args);
                                }
                                return Ok(RuntimeValue::Null);
                            }
                        }
                    }
                }

                // If we didn't jump and finished all instructions in block, return null
                if !jumped {
                    return Ok(RuntimeValue::Null);
                }
            }
        }
    }

    /// Handle method calls on objects
    /// Supports extended methods and built-in methods
    fn handle_method_call(&mut self, args: &[RuntimeValue]) -> Result<RuntimeValue, String> {
        if args.len() < 2 {
            return Ok(RuntimeValue::Null);
        }

        let obj = &args[0];
        let method_name = args[1].as_string();

        // Build method args: [obj, ...rest_args]
        let method_args: Vec<RuntimeValue> = std::iter::once(obj.clone())
            .chain(args.iter().skip(2).cloned())
            .collect();

        if let RuntimeValue::U64(arc_id) = obj {
            match method_name.as_str() {
                "strong_count" => {
                    let count = self
                        .arc_strong_counts
                        .get(arc_id)
                        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(0);
                    return Ok(RuntimeValue::I64(count as i64));
                }
                "weak_count" => {
                    let count = self
                        .arc_weak_counts
                        .get(arc_id)
                        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(0);
                    return Ok(RuntimeValue::I64(count as i64));
                }
                "is_alive" => {
                    let count = self
                        .arc_strong_counts
                        .get(arc_id)
                        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(0);
                    return Ok(RuntimeValue::Bool(count > 0));
                }
                "upgrade" => {
                    let count = self
                        .arc_strong_counts
                        .get(arc_id)
                        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(0);
                    if count > 0 {
                        if let Some(sc) = self.arc_strong_counts.get(arc_id) {
                            sc.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        return Ok(RuntimeValue::U64(*arc_id));
                    } else {
                        return Ok(RuntimeValue::Null);
                    }
                }
                _ => {}
            }
        }

        // Check if object has __class__ field and if there's an extended method
        if let RuntimeValue::Object(obj_map) = obj {
            if let Some(RuntimeValue::String(class_name)) = obj_map.get("__class__") {
                let cache_key = (class_name.clone(), method_name.clone());

                // 1. Check cache first
                if let Some(callee) = self.extended_method_cache.get(&cache_key).cloned() {
                    return self.execute_function_with_this(
                        &callee, // Already cloned when inserted
                        method_args.iter().skip(1).cloned().collect(),
                        obj.clone(),
                    );
                }

                // 2. If not in cache, perform lookup
                if super::builtins::get_extended_method(class_name, &method_name).is_some() {
                    if let Some(callee) = self.functions.get(&method_name).cloned() {
                        // 3. Add to cache for next time
                        self.extended_method_cache.insert(cache_key, callee.clone());

                        return self.execute_function_with_this(
                            &callee,
                            method_args.iter().skip(1).cloned().collect(),
                            obj.clone(),
                        );
                    }
                }
            }
        }

        // Fall back to builtin method - call runtime_call_method directly via __call_method
        if let Some(builtin) = self.builtins.get("__call_method") {
            return Ok(builtin(args));
        }

        // Final fallback: check if it's a field containing a function
        if let RuntimeValue::Object(obj_map) = obj {
            if let Some(RuntimeValue::Function(func)) = obj_map.get(&method_name) {
                // It's a function field - call it indirectly
                let func_with_args = std::iter::once(RuntimeValue::Function(func.clone()))
                    .chain(args.iter().skip(2).cloned())
                    .collect::<Vec<_>>();
                return self.handle_call_indirect(&func_with_args);
            }
        }

        Ok(RuntimeValue::Null)
    }

    /// Execute a function with 'this' bound to an object
    fn execute_function_with_this(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
        this_val: RuntimeValue,
    ) -> Result<RuntimeValue, String> {
        // Check recursion depth
        self.recursion_depth += 1;
        if self.recursion_depth > self.max_recursion {
            self.recursion_depth -= 1;
            return Err(format!(
                "Maximum recursion depth ({}) exceeded",
                self.max_recursion
            ));
        }

        let mut frame = JitFrame::new();

        // Bind 'this'
        frame.set_var("this".to_string(), this_val.clone());

        // Bind arguments to parameters
        for (i, (name, _ty)) in func.params.iter().enumerate() {
            let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
            frame.set_var(name.clone(), value);
        }

        // Execute blocks
        let mut current_block = func.entry_block;

        loop {
            let block = func
                .get_block(current_block)
                .ok_or_else(|| format!("Block {} not found", current_block))?;

            for inst in &block.instructions {
                match self.execute_instruction(&mut frame, inst, func)? {
                    ControlFlow::Next => {}
                    ControlFlow::Jump(target) => {
                        current_block = target;
                        break;
                    }
                    ControlFlow::Return(value) => {
                        self.recursion_depth -= 1;
                        return Ok(value);
                    }
                    ControlFlow::TailCall(name, new_args) => {
                        // TCO in method context - rare but handle it
                        self.recursion_depth -= 1;
                        if let Some(target_func) = self.functions.get(&name).cloned() {
                            return self.execute_function(&target_func, new_args);
                        }
                        return Ok(RuntimeValue::Null);
                    }
                }
            }
        }
    }

    /// Execute a single instruction
    fn execute_instruction(
        &mut self,
        frame: &mut JitFrame,
        inst: &LirInst,
        _func: &LirFunction,
    ) -> Result<ControlFlow, String> {
        match inst {
            LirInst::ConstI64(dst, n) => {
                frame.set_value(*dst, RuntimeValue::Int(*n));
                Ok(ControlFlow::Next)
            }

            LirInst::ConstF64(dst, n) => {
                frame.set_value(*dst, RuntimeValue::F64(*n));
                Ok(ControlFlow::Next)
            }

            // Fixed-width unsigned integer constants
            LirInst::ConstU8(dst, n) => {
                frame.set_value(*dst, RuntimeValue::U8(*n));
                Ok(ControlFlow::Next)
            }
            LirInst::ConstU16(dst, n) => {
                frame.set_value(*dst, RuntimeValue::U16(*n));
                Ok(ControlFlow::Next)
            }
            LirInst::ConstU32(dst, n) => {
                frame.set_value(*dst, RuntimeValue::U32(*n));
                Ok(ControlFlow::Next)
            }
            LirInst::ConstU64(dst, n) => {
                frame.set_value(*dst, RuntimeValue::U64(*n));
                Ok(ControlFlow::Next)
            }
            LirInst::ConstU128(dst, n) => {
                frame.set_value(*dst, RuntimeValue::U128(*n));
                Ok(ControlFlow::Next)
            }
            // Fixed-width signed integer constants
            LirInst::ConstI8(dst, n) => {
                frame.set_value(*dst, RuntimeValue::I8(*n));
                Ok(ControlFlow::Next)
            }
            LirInst::ConstI16(dst, n) => {
                frame.set_value(*dst, RuntimeValue::I16(*n));
                Ok(ControlFlow::Next)
            }
            LirInst::ConstI32(dst, n) => {
                frame.set_value(*dst, RuntimeValue::I32(*n));
                Ok(ControlFlow::Next)
            }
            LirInst::ConstI128(dst, n) => {
                frame.set_value(*dst, RuntimeValue::I128(*n));
                Ok(ControlFlow::Next)
            }
            // Fixed-width float constants
            LirInst::ConstF32(dst, n) => {
                frame.set_value(*dst, RuntimeValue::F32(*n));
                Ok(ControlFlow::Next)
            }

            LirInst::ConstBool(dst, b) => {
                frame.set_value(*dst, RuntimeValue::Bool(*b));
                Ok(ControlFlow::Next)
            }

            LirInst::ConstString(dst, s) => {
                frame.set_value(*dst, RuntimeValue::String(s.clone()));
                Ok(ControlFlow::Next)
            }

            LirInst::ConstNull(dst) => {
                frame.set_value(*dst, RuntimeValue::Null);
                Ok(ControlFlow::Next)
            }

            LirInst::ConstBigInt(dst, bi) => {
                frame.set_value(*dst, RuntimeValue::BigInt(bi.clone()));
                Ok(ControlFlow::Next)
            }

            LirInst::ConstFunc(dst, name, captured_var_names, is_async) => {
                // Build captures map from current frame/globals
                let mut captures = FastMap::default();
                for var_name in captured_var_names {
                    if let Some(val) = frame
                        .get_var(var_name)
                        .or_else(|| self.globals.get(var_name).cloned())
                    {
                        captures.insert(var_name.clone(), val);
                    }
                }
                frame.set_value(
                    *dst,
                    RuntimeValue::Function(CallableFunction {
                        name: name.clone(),
                        params: vec![],
                        captures,
                        is_async: *is_async,
                    }),
                );
                Ok(ControlFlow::Next)
            }

            LirInst::LoadVar(dst, name) => {
                let value = frame
                    .get_var(name)
                    .or_else(|| self.globals.get(name).cloned())
                    .or_else(|| {
                        // Check if name refers to a function
                        if self.functions.contains_key(name) {
                            Some(RuntimeValue::Function(CallableFunction {
                                name: name.clone(),
                                params: vec![],
                                captures: FastMap::default(),
                                is_async: self
                                    .functions
                                    .get(name)
                                    .map(|f| f.is_async)
                                    .unwrap_or(false),
                            }))
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| format!("Undefined variable '{}'", name))?;
                frame.set_value(*dst, value);
                Ok(ControlFlow::Next)
            }

            LirInst::StoreVar(name, src) => {
                let value = frame.get_value(*src);
                frame.set_var(name.clone(), value.clone());
                // Also store to globals so variables are accessible from nested functions
                // This is needed because top-level variables in main() need to be accessible
                // from async functions and closures
                self.globals.insert(name.clone(), value);
                Ok(ControlFlow::Next)
            }

            LirInst::LoadModule(dst, alias) => {
                // Load the imported module and get its namespace
                let namespace = self.load_imported_module(alias)?;
                frame.set_value(*dst, namespace.clone());
                // Also store in globals so nested functions can access it
                self.globals.insert(alias.clone(), namespace);
                Ok(ControlFlow::Next)
            }

            LirInst::Copy(dst, src) => {
                let value = frame.get_value(*src);
                frame.set_value(*dst, value);
                Ok(ControlFlow::Next)
            }

            // Arithmetic operations (BigInt-aware)
            LirInst::AddI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    RuntimeValue::BigInt(av_bi + bv_bi)
                } else {
                    RuntimeValue::Int(av.as_int().unwrap_or(0) + bv.as_int().unwrap_or(0))
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::AddF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                let result = av + bv;
                frame.set_value_f64(*dst, result);
                frame.set_value(*dst, RuntimeValue::Float(result));
                Ok(ControlFlow::Next)
            }

            LirInst::SubI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    RuntimeValue::BigInt(av_bi - bv_bi)
                } else {
                    RuntimeValue::Int(av.as_int().unwrap_or(0) - bv.as_int().unwrap_or(0))
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::SubF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                let result = av - bv;
                frame.set_value_f64(*dst, result);
                frame.set_value(*dst, RuntimeValue::Float(result));
                Ok(ControlFlow::Next)
            }

            LirInst::MulI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    RuntimeValue::BigInt(av_bi * bv_bi)
                } else {
                    RuntimeValue::Int(av.as_int().unwrap_or(0) * bv.as_int().unwrap_or(0))
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::MulF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                let result = av * bv;
                frame.set_value_f64(*dst, result);
                frame.set_value(*dst, RuntimeValue::Float(result));
                Ok(ControlFlow::Next)
            }

            LirInst::DivI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(1));
                    if bv_bi != BigInt::from(0) {
                        RuntimeValue::BigInt(av_bi / bv_bi)
                    } else {
                        RuntimeValue::BigInt(BigInt::from(0))
                    }
                } else {
                    let bv_i = bv.as_int().unwrap_or(1);
                    RuntimeValue::Int(if bv_i != 0 {
                        av.as_int().unwrap_or(0) / bv_i
                    } else {
                        0
                    })
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::DivF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                let result = if bv != 0.0 { av / bv } else { 0.0 };
                frame.set_value_f64(*dst, result);
                frame.set_value(*dst, RuntimeValue::Float(result));
                Ok(ControlFlow::Next)
            }

            LirInst::ModI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(1));
                    if bv_bi != BigInt::from(0) {
                        RuntimeValue::BigInt(av_bi % bv_bi)
                    } else {
                        RuntimeValue::BigInt(BigInt::from(0))
                    }
                } else {
                    let bv_i = bv.as_int().unwrap_or(1);
                    RuntimeValue::Int(if bv_i != 0 {
                        av.as_int().unwrap_or(0) % bv_i
                    } else {
                        0
                    })
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::NegI64(dst, a) => {
                let av = frame.get_value(*a);
                let result = if av.is_bigint() {
                    RuntimeValue::BigInt(-av.as_bigint().unwrap_or(BigInt::from(0)))
                } else {
                    RuntimeValue::Int(-av.as_int().unwrap_or(0))
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::NegF64(dst, a) => {
                // BLAZING FAST: Use f64 slots directly
                let result = -frame.get_value_f64(*a);
                frame.set_value_f64(*dst, result);
                frame.set_value(*dst, RuntimeValue::Float(result));
                Ok(ControlFlow::Next)
            }

            LirInst::Not(dst, a) => {
                let av = frame.get_value(*a).as_bool().unwrap_or(false);
                frame.set_value(*dst, RuntimeValue::Bool(!av));
                Ok(ControlFlow::Next)
            }

            // Comparison operations (BigInt-aware)
            LirInst::CmpLtI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    av_bi < bv_bi
                } else {
                    av.as_int().unwrap_or(0) < bv.as_int().unwrap_or(0)
                };
                frame.set_value(*dst, RuntimeValue::Bool(result));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpLeI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    av_bi <= bv_bi
                } else {
                    av.as_int().unwrap_or(0) <= bv.as_int().unwrap_or(0)
                };
                frame.set_value(*dst, RuntimeValue::Bool(result));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpGtI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    av_bi > bv_bi
                } else {
                    av.as_int().unwrap_or(0) > bv.as_int().unwrap_or(0)
                };
                frame.set_value(*dst, RuntimeValue::Bool(result));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpGeI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    av_bi >= bv_bi
                } else {
                    av.as_int().unwrap_or(0) >= bv.as_int().unwrap_or(0)
                };
                frame.set_value(*dst, RuntimeValue::Bool(result));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpEqI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    av_bi == bv_bi
                } else {
                    av.as_int().unwrap_or(0) == bv.as_int().unwrap_or(0)
                };
                frame.set_value(*dst, RuntimeValue::Bool(result));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpNeI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    av_bi != bv_bi
                } else {
                    av.as_int().unwrap_or(0) != bv.as_int().unwrap_or(0)
                };
                frame.set_value(*dst, RuntimeValue::Bool(result));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpLtF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                frame.set_value(*dst, RuntimeValue::Bool(av < bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpLeF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                frame.set_value(*dst, RuntimeValue::Bool(av <= bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpGtF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                frame.set_value(*dst, RuntimeValue::Bool(av > bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpGeF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                frame.set_value(*dst, RuntimeValue::Bool(av >= bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpEqF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                frame.set_value(*dst, RuntimeValue::Bool((av - bv).abs() < f64::EPSILON));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpNeF64(dst, a, b) => {
                // BLAZING FAST: Use f64 slots directly
                let av = frame.get_value_f64(*a);
                let bv = frame.get_value_f64(*b);
                frame.set_value(*dst, RuntimeValue::Bool((av - bv).abs() >= f64::EPSILON));
                Ok(ControlFlow::Next)
            }

            // Logical operations
            LirInst::And(dst, a, b) => {
                let av = frame.get_value(*a).as_bool().unwrap_or(false);
                let bv = frame.get_value(*b).as_bool().unwrap_or(false);
                frame.set_value(*dst, RuntimeValue::Bool(av && bv));
                Ok(ControlFlow::Next)
            }

            LirInst::Or(dst, a, b) => {
                let av = frame.get_value(*a).as_bool().unwrap_or(false);
                let bv = frame.get_value(*b).as_bool().unwrap_or(false);
                frame.set_value(*dst, RuntimeValue::Bool(av || bv));
                Ok(ControlFlow::Next)
            }

            // Bitwise operations
            LirInst::BitAnd(dst, a, b) => {
                let av = frame.get_value(*a).as_int().unwrap_or(0);
                let bv = frame.get_value(*b).as_int().unwrap_or(0);
                frame.set_value(*dst, RuntimeValue::Int(av & bv));
                Ok(ControlFlow::Next)
            }

            LirInst::BitOr(dst, a, b) => {
                let av = frame.get_value(*a).as_int().unwrap_or(0);
                let bv = frame.get_value(*b).as_int().unwrap_or(0);
                frame.set_value(*dst, RuntimeValue::Int(av | bv));
                Ok(ControlFlow::Next)
            }

            LirInst::BitXor(dst, a, b) => {
                let av = frame.get_value(*a).as_int().unwrap_or(0);
                let bv = frame.get_value(*b).as_int().unwrap_or(0);
                frame.set_value(*dst, RuntimeValue::Int(av ^ bv));
                Ok(ControlFlow::Next)
            }

            LirInst::Shl(dst, a, b) => {
                let av = frame.get_value(*a).as_int().unwrap_or(0);
                let bv = frame.get_value(*b).as_int().unwrap_or(0);
                frame.set_value(*dst, RuntimeValue::Int(av << bv));
                Ok(ControlFlow::Next)
            }

            LirInst::Shr(dst, a, b) => {
                let av = frame.get_value(*a).as_int().unwrap_or(0);
                let bv = frame.get_value(*b).as_int().unwrap_or(0);
                frame.set_value(*dst, RuntimeValue::Int(av >> bv));
                Ok(ControlFlow::Next)
            }

            // Type conversions
            LirInst::I64ToF64(dst, src) => {
                let v = frame.get_value(*src).as_int().unwrap_or(0);
                frame.set_value(*dst, RuntimeValue::Float(v as f64));
                Ok(ControlFlow::Next)
            }

            LirInst::F64ToI64(dst, src) => {
                let v = frame.get_value(*src).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Int(v as i64));
                Ok(ControlFlow::Next)
            }

            // Function calls
            LirInst::Call(dst, name, args) => {
                let arg_values: Vec<RuntimeValue> =
                    args.iter().map(|a| frame.get_value(*a)).collect();

                let result = if let Some(target_func) = self.functions.get(name).cloned() {
                    // Call a named function in the module
                    if target_func.is_async {
                        // For async functions, wrap in a Promise
                        self.call_async_function(&target_func, arg_values, &FastMap::default())?
                    } else {
                        self.execute_function(&target_func, arg_values)?
                    }
                } else if let Some(RuntimeValue::Function(func_val)) = frame.get_var(name) {
                    // Call a function stored in a variable (e.g., resolve/reject callbacks, closures)
                    match func_val.name.as_str() {
                        "__resolve" => {
                            if let Some(RuntimeValue::Int(id)) =
                                func_val.captures.get("__promise_id")
                            {
                                let value =
                                    arg_values.first().cloned().unwrap_or(RuntimeValue::Null);
                                PROMISE_RUNTIME.fulfill_value(*id as u64, value);
                            }
                            RuntimeValue::Null
                        }
                        "__reject" => {
                            if let Some(RuntimeValue::Int(id)) =
                                func_val.captures.get("__promise_id")
                            {
                                let reason =
                                    arg_values.first().cloned().unwrap_or(RuntimeValue::Null);
                                PROMISE_RUNTIME.reject_value(*id as u64, reason);
                            }
                            RuntimeValue::Null
                        }
                        other_name => {
                            // Try to call as a regular function with captured variables
                            if let Some(target_func) = self.functions.get(other_name).cloned() {
                                // Check if this is an async function
                                if func_val.is_async {
                                    self.call_async_function(
                                        &target_func,
                                        arg_values,
                                        &func_val.captures,
                                    )?
                                } else {
                                    self.execute_function_with_captures(
                                        &target_func,
                                        arg_values,
                                        &func_val.captures,
                                    )?
                                }
                            } else {
                                RuntimeValue::Null
                            }
                        }
                    }
                } else {
                    // Check if it's an extern FFI function
                    use crate::backends::ffi_import::{call_foreign, lookup_function};
                    use crate::parsing::ast::Value;

                    if let Some(func) = lookup_function(name) {
                        // Convert RuntimeValue to Value for FFI call
                        let args: Result<Vec<Value>, String> = arg_values
                            .iter()
                            .map(|rv| match rv {
                                RuntimeValue::Int(i) => Ok(Value::I64(*i)),
                                RuntimeValue::Float(f) => Ok(Value::F64(*f)),
                                RuntimeValue::Bool(b) => Ok(Value::Bool(*b)),
                                RuntimeValue::String(s) => Ok(Value::Str(s.clone())),
                                RuntimeValue::I8(n) => Ok(Value::I8(*n)),
                                RuntimeValue::I16(n) => Ok(Value::I16(*n)),
                                RuntimeValue::I32(n) => Ok(Value::I32(*n)),
                                RuntimeValue::I64(n) => Ok(Value::I64(*n)),
                                RuntimeValue::I128(n) => Ok(Value::I128(*n)),
                                RuntimeValue::U8(n) => Ok(Value::U8(*n)),
                                RuntimeValue::U16(n) => Ok(Value::U16(*n)),
                                RuntimeValue::U32(n) => Ok(Value::U32(*n)),
                                RuntimeValue::U64(n) => Ok(Value::U64(*n)),
                                RuntimeValue::U128(n) => Ok(Value::U128(*n)),
                                RuntimeValue::F32(n) => Ok(Value::F32(*n)),
                                RuntimeValue::F64(n) => Ok(Value::F64(*n)),
                                RuntimeValue::BigInt(bi) => Ok(Value::BigInt(bi.clone())),
                                RuntimeValue::Null => Ok(Value::Null),
                                _ => Err(format!("Cannot pass {:?} to FFI", rv)),
                            })
                            .collect();

                        match args {
                            Ok(ffi_args) => match call_foreign(&func, &ffi_args) {
                                Ok(Value::I64(n)) => RuntimeValue::Int(n),
                                Ok(Value::F64(n)) => RuntimeValue::Float(n),
                                Ok(Value::F32(n)) => RuntimeValue::F32(n),
                                Ok(Value::I8(n)) => RuntimeValue::I8(n),
                                Ok(Value::I16(n)) => RuntimeValue::I16(n),
                                Ok(Value::I32(n)) => RuntimeValue::I32(n),
                                Ok(Value::I128(n)) => RuntimeValue::I128(n),
                                Ok(Value::U8(n)) => RuntimeValue::U8(n),
                                Ok(Value::U16(n)) => RuntimeValue::U16(n),
                                Ok(Value::U32(n)) => RuntimeValue::U32(n),
                                Ok(Value::U64(n)) => RuntimeValue::U64(n),
                                Ok(Value::U128(n)) => RuntimeValue::U128(n),
                                Ok(Value::Bool(b)) => RuntimeValue::Bool(b),
                                Ok(Value::Str(s)) => RuntimeValue::String(s),
                                Ok(Value::BigInt(bi)) => RuntimeValue::BigInt(bi),
                                Ok(Value::Null) => RuntimeValue::Null,
                                Ok(_) => RuntimeValue::Null,
                                Err(e) => {
                                    return Err(format!("FFI call error for '{}': {}", name, e));
                                }
                            },
                            Err(e) => return Err(e),
                        }
                    } else {
                        RuntimeValue::Null
                    }
                };

                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::CallBuiltin(dst, name, args) => {
                let arg_values: Vec<RuntimeValue> =
                    args.iter().map(|a| frame.get_value(*a)).collect();

                // Pointer-aware fast path for get_index/set_index
                if name == "get_index" {
                    if let Some(RuntimeValue::U64(ptr)) = arg_values.get(0) {
                        if let Some(idx_rv) = arg_values.get(1) {
                            let off = idx_rv
                                .as_int()
                                .ok_or_else(|| "index must be integer".to_string())?;
                            if off < 0 {
                                return Err("index must be non-negative".to_string());
                            }
                            let off_usize = off as usize;

                            // Use typed load if available, fallback to byte load
                            // For now, assume byte-level access (typed loads will be added when type info propagates)
                            let byte = unsafe_heap::load_u8(*ptr, off_usize)?;
                            frame.set_value(*dst, RuntimeValue::Int(byte as i64));
                            return Ok(ControlFlow::Next);
                        }
                    }
                } else if name == "set_index" {
                    if arg_values.len() >= 3 {
                        if let Some(RuntimeValue::U64(ptr)) = arg_values.get(0) {
                            let off = arg_values[1]
                                .as_int()
                                .ok_or_else(|| "index must be integer".to_string())?;
                            if off < 0 {
                                return Err("index must be non-negative".to_string());
                            }
                            let off_usize = off as usize;
                            let byte_val = arg_values[2]
                                .as_int()
                                .ok_or_else(|| "value must be integer".to_string())?;
                            let byte_u8 = if byte_val >= 0 && byte_val <= 255 {
                                byte_val as u8
                            } else {
                                return Err("value must be 0..255".to_string());
                            };

                            // State validation happens inside unsafe_heap::store_u8
                            unsafe_heap::store_u8(*ptr, off_usize, byte_u8)?;
                            frame.set_value(*dst, RuntimeValue::U64(*ptr));
                            return Ok(ControlFlow::Next);
                        }
                    }
                }

                // Special handling for array methods with callbacks - these need access to compiled functions
                let result = if name == "map" && arg_values.len() >= 2 {
                    self.handle_array_map(&arg_values)?
                } else if name == "filter" && arg_values.len() >= 2 {
                    self.handle_array_filter(&arg_values)?
                } else if name == "reduce" && arg_values.len() >= 2 {
                    self.handle_array_reduce(&arg_values)?
                } else if name == "__method_then" || name == "__method_catch" {
                    self.handle_promise_then_catch(name, &arg_values)?
                } else if name == "await" {
                    // Handle await with microtask processing
                    self.handle_await(&arg_values)?
                } else if name == "call_indirect" {
                    // Handle indirect function calls (e.g., m.func(...))
                    self.handle_call_indirect(&arg_values)?
                } else if name == "__call_method" && arg_values.len() >= 2 {
                    // Special handling for __call_method to support extended methods and instance methods
                    self.handle_method_call(&arg_values)?
                } else if name == "str_concat" && arg_values.len() >= 2 {
                    // Polymorphic concatenation - works like the interpreter's Plus operator
                    // If both values are numeric, do numeric addition
                    // Otherwise, convert to strings and concatenate
                    let left_val = &arg_values[0];
                    let right_val = &arg_values[1];

                    // Check if both are numeric types
                    let left_is_numeric = matches!(
                        left_val,
                        RuntimeValue::Int(_)
                            | RuntimeValue::Float(_)
                            | RuntimeValue::BigInt(_)
                            | RuntimeValue::U8(_)
                            | RuntimeValue::U16(_)
                            | RuntimeValue::U32(_)
                            | RuntimeValue::U64(_)
                            | RuntimeValue::U128(_)
                            | RuntimeValue::I8(_)
                            | RuntimeValue::I16(_)
                            | RuntimeValue::I32(_)
                            | RuntimeValue::I64(_)
                            | RuntimeValue::I128(_)
                            | RuntimeValue::F32(_)
                            | RuntimeValue::F64(_)
                    );
                    let right_is_numeric = matches!(
                        right_val,
                        RuntimeValue::Int(_)
                            | RuntimeValue::Float(_)
                            | RuntimeValue::BigInt(_)
                            | RuntimeValue::U8(_)
                            | RuntimeValue::U16(_)
                            | RuntimeValue::U32(_)
                            | RuntimeValue::U64(_)
                            | RuntimeValue::U128(_)
                            | RuntimeValue::I8(_)
                            | RuntimeValue::I16(_)
                            | RuntimeValue::I32(_)
                            | RuntimeValue::I64(_)
                            | RuntimeValue::I128(_)
                            | RuntimeValue::F32(_)
                            | RuntimeValue::F64(_)
                    );

                    // If either is a string or array, do string/array concatenation
                    if matches!(left_val, RuntimeValue::String(_) | RuntimeValue::Array(_))
                        || matches!(right_val, RuntimeValue::String(_) | RuntimeValue::Array(_))
                    {
                        // String/Array concatenation
                        if matches!(
                            left_val,
                            RuntimeValue::Array(_) | RuntimeValue::DynArray { .. }
                        ) && matches!(
                            right_val,
                            RuntimeValue::Array(_) | RuntimeValue::DynArray { .. }
                        ) {
                            // Array concatenation
                            let mut left_arr = match left_val {
                                RuntimeValue::Array(a) => a.clone(),
                                RuntimeValue::DynArray { data, .. } => data.clone(),
                                _ => vec![],
                            };
                            let right_arr = match right_val {
                                RuntimeValue::Array(a) => a.clone(),
                                RuntimeValue::DynArray { data, .. } => data.clone(),
                                _ => vec![],
                            };
                            left_arr.extend(right_arr);
                            RuntimeValue::Array(left_arr)
                        } else {
                            // String concatenation
                            let left_str = match left_val {
                                RuntimeValue::String(s) => s.clone(),
                                other => other.as_string(),
                            };
                            let right_str = match right_val {
                                RuntimeValue::String(s) => s.clone(),
                                other => other.as_string(),
                            };
                            let mut result =
                                String::with_capacity(left_str.len() + right_str.len());
                            result.push_str(&left_str);
                            result.push_str(&right_str);
                            RuntimeValue::String(result)
                        }
                    } else if left_is_numeric && right_is_numeric {
                        // Numeric addition
                        if let (Some(l), Some(r)) = (left_val.as_float(), right_val.as_float()) {
                            RuntimeValue::Float(l + r)
                        } else {
                            RuntimeValue::Int(
                                left_val.as_int().unwrap_or(0) + right_val.as_int().unwrap_or(0),
                            )
                        }
                    } else {
                        // Mixed types - convert to strings and concatenate
                        let left_str = left_val.as_string();
                        let right_str = right_val.as_string();
                        let mut result = String::with_capacity(left_str.len() + right_str.len());
                        result.push_str(&left_str);
                        result.push_str(&right_str);
                        RuntimeValue::String(result)
                    }
                } else if let Some(builtin) = self.builtins.get(name) {
                    builtin(&arg_values)
                } else {
                    RuntimeValue::Null
                };

                // Halt execution if a builtin recorded a fatal error (e.g., input<T> bounds)
                if let Some(err) = super::builtins::take_jit_error() {
                    return Err(err);
                }

                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            // Call builtin with generic type parameter (for input<T>())
            LirInst::CallBuiltinGeneric(dst, name, args, generic_type) => {
                let arg_values: Vec<RuntimeValue> =
                    args.iter().map(|a| frame.get_value(*a)).collect();

                // Set generic type context for the builtin call
                super::builtins::set_jit_generic_type(generic_type.clone());

                let result = if let Some(builtin) = self.builtins.get(name) {
                    builtin(&arg_values)
                } else {
                    RuntimeValue::Null
                };

                // Clear generic type context after use
                super::builtins::clear_jit_generic_type();

                // Halt execution if a builtin recorded a fatal error (e.g., input<T> bounds)
                if let Some(err) = super::builtins::take_jit_error() {
                    return Err(err);
                }

                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            // Control flow
            LirInst::Jump(target) => Ok(ControlFlow::Jump(*target)),

            LirInst::JumpIf(cond, then_block, else_block) => {
                let cond_val = frame.get_value(*cond).as_bool().unwrap_or(false);
                if cond_val {
                    Ok(ControlFlow::Jump(*then_block))
                } else {
                    Ok(ControlFlow::Jump(*else_block))
                }
            }

            LirInst::Return(value) => {
                let ret_value = value
                    .map(|v| frame.get_value(v))
                    .unwrap_or(RuntimeValue::Null);
                Ok(ControlFlow::Return(ret_value))
            }

            LirInst::TailCall(name, args) => {
                // Collect argument values
                let arg_values: Vec<RuntimeValue> =
                    args.iter().map(|&a| frame.get_value(a)).collect();
                Ok(ControlFlow::TailCall(name.clone(), arg_values))
            }

            // Memory management operations
            LirInst::ArcNew(dst, value) => {
                let value_obj = frame.get_value(*value);
                let arc_id = self.next_arc_id;
                self.next_arc_id += 1;
                self.arc_values
                    .insert(arc_id, Arc::new(std::sync::Mutex::new(value_obj.clone())));
                self.arc_strong_counts
                    .insert(arc_id, Arc::new(std::sync::atomic::AtomicUsize::new(1)));
                self.arc_weak_counts
                    .insert(arc_id, Arc::new(std::sync::atomic::AtomicUsize::new(0)));
                crate::backends::common::builtins::objects::insert_jit_arc_value(arc_id, value_obj);
                frame.set_value(*dst, RuntimeValue::U64(arc_id));
                Ok(ControlFlow::Next)
            }

            LirInst::ArcClone(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(strong_count) = self.arc_strong_counts.get(&arc_id) {
                        let count = strong_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if count == usize::MAX {
                            return Err("ARC reference count overflow".to_string());
                        }
                        frame.set_value(*dst, arc_id_rv);
                        Ok(ControlFlow::Next)
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("ArcClone requires ArcId as argument".to_string())
                }
            }

            LirInst::ArcDrop(arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(strong_count) = self.arc_strong_counts.get(&arc_id) {
                        let prev_count =
                            strong_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        if prev_count == 1 {
                            let weak_count = self
                                .arc_weak_counts
                                .get(&arc_id)
                                .map(|wc| wc.load(std::sync::atomic::Ordering::Relaxed))
                                .unwrap_or(0);
                            if weak_count == 0 {
                                self.arc_values.remove(&arc_id);
                                self.arc_strong_counts.remove(&arc_id);
                                self.arc_weak_counts.remove(&arc_id);
                                crate::backends::common::builtins::objects::remove_jit_arc_value(
                                    arc_id,
                                );
                            }
                        }
                        Ok(ControlFlow::Next)
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("ArcDrop requires ArcId as argument".to_string())
                }
            }

            LirInst::WeakNew(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(weak_count) = self.arc_weak_counts.get(&arc_id) {
                        let count = weak_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if count == usize::MAX {
                            return Err("Weak reference count overflow".to_string());
                        }
                        frame.set_value(*dst, RuntimeValue::U64(arc_id));
                        Ok(ControlFlow::Next)
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("WeakNew requires ArcId as argument".to_string())
                }
            }

            LirInst::WeakDrop(arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(weak_count) = self.arc_weak_counts.get(&arc_id) {
                        let prev_count =
                            weak_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        if prev_count == 1 {
                            let strong_count = self
                                .arc_strong_counts
                                .get(&arc_id)
                                .map(|sc| sc.load(std::sync::atomic::Ordering::Relaxed))
                                .unwrap_or(0);
                            if strong_count == 0 {
                                self.arc_values.remove(&arc_id);
                                self.arc_strong_counts.remove(&arc_id);
                                self.arc_weak_counts.remove(&arc_id);
                            }
                        }
                        Ok(ControlFlow::Next)
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("WeakDrop requires ArcId as argument".to_string())
                }
            }

            LirInst::ArcGet(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(arc_value) = self.arc_values.get(&arc_id) {
                        if let Ok(guard) = arc_value.lock() {
                            frame.set_value(*dst, guard.clone());
                            Ok(ControlFlow::Next)
                        } else {
                            Err(format!("Failed to lock ARC value: {}", arc_id))
                        }
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("ArcGet requires ArcId as argument".to_string())
                }
            }

            LirInst::ArcSet(arc_id_val, value) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    let value_obj = frame.get_value(*value);
                    if let Some(arc_value) = self.arc_values.get(&arc_id) {
                        if let Ok(mut guard) = arc_value.lock() {
                            *guard = value_obj.clone();
                            crate::backends::common::builtins::objects::insert_jit_arc_value(
                                arc_id, value_obj,
                            );
                            Ok(ControlFlow::Next)
                        } else {
                            Err(format!("Failed to lock ARC value: {}", arc_id))
                        }
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("ArcSet requires ArcId as argument".to_string())
                }
            }

            LirInst::ArcStrongCount(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(strong_count) = self.arc_strong_counts.get(&arc_id) {
                        let count = strong_count.load(std::sync::atomic::Ordering::Relaxed);
                        frame.set_value(*dst, RuntimeValue::Int(count as i64));
                        Ok(ControlFlow::Next)
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("ArcStrongCount requires ArcId as argument".to_string())
                }
            }

            LirInst::ArcWeakCount(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(weak_count) = self.arc_weak_counts.get(&arc_id) {
                        let count = weak_count.load(std::sync::atomic::Ordering::Relaxed);
                        frame.set_value(*dst, RuntimeValue::Int(count as i64));
                        Ok(ControlFlow::Next)
                    } else {
                        Err(format!("Invalid ARC id: {}", arc_id))
                    }
                } else {
                    Err("ArcWeakCount requires ArcId as argument".to_string())
                }
            }

            LirInst::Alloc(dst, size_val) => {
                let size_rv = frame.get_value(*size_val);
                if let Some(size) = size_rv.as_int().map(|n| n as usize) {
                    let ptr = unsafe_heap::alloc(size)?;
                    frame.set_value(*dst, RuntimeValue::U64(ptr));
                    Ok(ControlFlow::Next)
                } else {
                    Err("Alloc requires size as integer argument".to_string())
                }
            }

            LirInst::AllocTyped(dst, size_val, elem_size) => {
                let size_rv = frame.get_value(*size_val);
                if let Some(size) = size_rv.as_int().map(|n| n as usize) {
                    if *elem_size <= 0 {
                        return Err("AllocTyped requires elem_size > 0".to_string());
                    }
                    let elem_size_usize = *elem_size as usize;
                    let ptr = unsafe_heap::alloc_typed(size, elem_size_usize)?;
                    frame.set_value(*dst, RuntimeValue::U64(ptr));
                    Ok(ControlFlow::Next)
                } else {
                    Err("AllocTyped requires size as integer argument".to_string())
                }
            }

            LirInst::Free(ptr_val) => {
                let ptr_rv = frame.get_value(*ptr_val);
                if let Some(ptr) = ptr_rv.as_int().map(|n| n as u64) {
                    unsafe_heap::free(ptr)?;
                    Ok(ControlFlow::Next)
                } else {
                    Err("Free requires pointer as argument".to_string())
                }
            }

            LirInst::PtrLoad(dst, ptr_val, index_val) => {
                let ptr_rv = frame.get_value(*ptr_val);
                let idx_rv = frame.get_value(*index_val);
                if let Some(ptr) = ptr_rv.as_int().map(|n| n as u64) {
                    let idx_i64 = idx_rv
                        .as_int()
                        .ok_or_else(|| "PtrLoad index must be an integer".to_string())?;
                    if idx_i64 < 0 {
                        return Err("PtrLoad index must be non-negative".to_string());
                    }
                    let idx = idx_i64 as usize;
                    let bytes = unsafe_heap::load_typed(ptr, idx)?;
                    // Interpret loaded bytes as little-endian i64 for now; exact type should be enforced by caller
                    let mut buf = [0u8; 8];
                    let copy_len = bytes.len().min(8);
                    buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                    let val = i64::from_le_bytes(buf);
                    frame.set_value(*dst, RuntimeValue::Int(val));
                    Ok(ControlFlow::Next)
                } else {
                    Err("PtrLoad requires pointer as argument".to_string())
                }
            }

            LirInst::PtrStore(ptr_val, value, index_val) => {
                let ptr_rv = frame.get_value(*ptr_val);
                let idx_rv = frame.get_value(*index_val);
                if let Some(ptr) = ptr_rv.as_int().map(|n| n as u64) {
                    let idx_i64 = idx_rv
                        .as_int()
                        .ok_or_else(|| "PtrStore index must be an integer".to_string())?;
                    if idx_i64 < 0 {
                        return Err("PtrStore index must be non-negative".to_string());
                    }
                    let idx = idx_i64 as usize;
                    let value_rv = frame.get_value(*value);
                    // Store as little-endian bytes based on pointer's elem_size
                    let elem_size = unsafe_heap::elem_size_of_ptr(ptr)?;
                    let mut buf = vec![0u8; elem_size];
                    let val_i64 = value_rv
                        .as_int()
                        .ok_or_else(|| "PtrStore requires integer-compatible value".to_string())?;
                    let bytes = val_i64.to_le_bytes();
                    let copy_len = elem_size.min(bytes.len());
                    buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                    unsafe_heap::store_typed(ptr, idx, &buf)?;
                    Ok(ControlFlow::Next)
                } else {
                    Err("PtrStore requires pointer as argument".to_string())
                }
            }

            LirInst::Phi(dst, values) => {
                // For interpreter-style JIT, we just take the first available value
                let value = values
                    .first()
                    .map(|(_, v)| frame.get_value(*v))
                    .unwrap_or(RuntimeValue::Null);
                frame.set_value(*dst, value);
                Ok(ControlFlow::Next)
            }
        }
    }
}

impl Default for JitContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Control flow result from instruction execution
enum ControlFlow {
    Next,
    Jump(BlockId),
    Return(RuntimeValue),
    /// Tail call optimization - restart function with new args
    TailCall(String, Vec<RuntimeValue>),
}
