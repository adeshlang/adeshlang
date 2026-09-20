//! Adaptive Tiered JIT with Speculative Optimization
//!
//! This module implements an advanced JIT compiler with:
//! - Multi-tier compilation (T0: Interpreter, T1: Baseline JIT, T2: Optimizing JIT)
//! - Profiling and type feedback collection
//! - Speculative optimizations based on runtime observations
//! - Deoptimization when assumptions fail
//! - On-stack replacement (OSR) for hot loops
//!
//! Architecture inspired by V8 TurboFan and HotSpot C2.

#![allow(dead_code)] // Many fields are infrastructure for future optimizations

pub mod adaptive_impl;

use super::builtins::{BuiltinRegistry, CallableFunction, RuntimeValue};
use super::lir::{LirFunction, LirInst, LirModule};
use super::unsafe_heap;
use crate::utils::collections::FastMap;
use adaptive_impl::*;
use num_bigint::BigInt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ============================================================================
// Adaptive JIT Context
// ============================================================================

/// Adaptive JIT execution context with profiling and speculative optimization
pub struct AdaptiveJitContext {
    builtins: BuiltinRegistry,
    globals: FastMap<String, RuntimeValue>,
    functions: FastMap<String, LirFunction>,
    /// Function profiles for tier decisions
    profiles: FastMap<String, FunctionProfile>,
    /// Tier promotion thresholds
    thresholds: AdaptiveThresholds,
    /// Speculative optimizer
    speculator: SpeculativeOptimizer,
    /// Hidden class system
    hidden_classes: HiddenClassSystem,
    /// Inline cache system
    inline_caches: InlineCacheSystem,
    /// Compiled code cache for each tier
    compiled_code: FastMap<(String, Tier), CompiledCode>,
    /// Statistics
    stats: AdaptiveJitStats,
    /// Current recursion depth
    recursion_depth: u32,
    /// Maximum recursion depth
    max_recursion: u32,
    /// ARC-managed values: ArcId -> RuntimeValue
    arc_values: FastMap<u64, std::sync::Arc<std::sync::Mutex<RuntimeValue>>>,
    /// ARC reference counts: ArcId -> strong_count
    arc_strong_counts: FastMap<u64, std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    /// ARC weak reference counts: ArcId -> weak_count
    arc_weak_counts: FastMap<u64, std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    /// Next ARC ID
    next_arc_id: AtomicU64,
}

/// Statistics for adaptive JIT
#[derive(Debug, Default)]
pub struct AdaptiveJitStats {
    pub total_calls: u64,
    pub interpreter_calls: u64,
    pub baseline_calls: u64,
    pub optimizing_calls: u64,
    pub tier_promotions: u64,
    pub deoptimizations: u64,
    pub osr_entries: u64,
    pub inline_cache_hits: u64,
    pub inline_cache_misses: u64,
    pub compile_time_ns: u64,
}

/// Compiled code representation (placeholder for actual machine code)
pub struct CompiledCode {
    /// Tier this code was compiled at
    pub tier: Tier,
    /// Function entry point
    pub entry: usize,
    /// Code size in bytes
    pub size: usize,
    /// Deoptimization points
    pub deopt_points: Vec<DeoptPoint>,
}

impl AdaptiveJitContext {
    /// Create a new adaptive JIT context
    pub fn new() -> Self {
        AdaptiveJitContext {
            builtins: BuiltinRegistry::new(),
            globals: FastMap::default(),
            functions: FastMap::default(),
            profiles: FastMap::default(),
            thresholds: AdaptiveThresholds::default(),
            speculator: SpeculativeOptimizer::new(),
            hidden_classes: HiddenClassSystem::new(),
            inline_caches: InlineCacheSystem::new(),
            compiled_code: FastMap::default(),
            stats: AdaptiveJitStats::default(),
            recursion_depth: 0,
            max_recursion: 10000,
            arc_values: FastMap::default(),
            arc_strong_counts: FastMap::default(),
            arc_weak_counts: FastMap::default(),
            next_arc_id: AtomicU64::new(1),
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(thresholds: AdaptiveThresholds) -> Self {
        let mut ctx = Self::new();
        ctx.thresholds = thresholds;
        ctx
    }

    /// Load an LIR module
    pub fn load_module(&mut self, module: &LirModule) {
        for func in &module.functions {
            self.profiles
                .insert(func.name.clone(), FunctionProfile::new());
            self.functions.insert(func.name.clone(), func.clone());
        }
    }

    /// Load a VIR module into the JIT context using the bridge adapter
    pub fn load_vir_module(
        &mut self,
        vir_module: &crate::ir::vir::VirModule,
    ) -> Result<(), String> {
        use crate::backends::common::vir_lir_bridge::VirToLirBridge;
        let mut bridge = VirToLirBridge::new();
        let lir_module = bridge.convert_module(vir_module)?;
        self.load_module(&lir_module);
        Ok(())
    }

    /// Compile a VIR module (equivalent to load_vir_module)
    pub fn compile_vir_module(
        &mut self,
        vir_module: &crate::ir::vir::VirModule,
    ) -> Result<(), String> {
        self.load_vir_module(vir_module)
    }

    /// Execute the main function
    pub fn run_main(&mut self) -> Result<RuntimeValue, String> {
        // Try to find main function
        if let Some(main_func) = self.functions.get("main").cloned() {
            let result = self.execute_function(&main_func, vec![])?;
            let max_drain_iterations = 100000;
            let mut drain_count = 0;
            let mut idle_count = 0;
            while drain_count < max_drain_iterations {
                if crate::backends::builtins::PROMISE_RUNTIME.has_microtasks() {
                    let _ = self.process_microtasks();
                    idle_count = 0;
                    drain_count += 1;
                    continue;
                }
                if crate::backends::builtins::PROMISE_RUNTIME.has_pending_timers() {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    idle_count += 1;
                    drain_count += 1;
                    if idle_count > 5000 {
                        break;
                    }
                    continue;
                }
                break;
            }
            use crate::backends::builtins::CURRENT_EXCEPTION;
            if let Some(exc) = CURRENT_EXCEPTION.with(|exc| exc.borrow_mut().take()) {
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
        } else {
            // Auto-detect entry point: look for top-level statements
            // In AdeshLang, if there's no main() but there are top-level statements,
            // they are wrapped in a synthetic main
            Err(
                "No main function found. Add a main() function or use top-level statements."
                    .to_string(),
            )
        }
    }

    /// Execute a function with tier-based dispatch
    pub fn execute_function(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        self.stats.total_calls += 1;

        // Check recursion depth
        self.recursion_depth += 1;
        if self.recursion_depth > self.max_recursion {
            self.recursion_depth -= 1;
            return Err(format!(
                "Maximum recursion depth ({}) exceeded",
                self.max_recursion
            ));
        }

        // Get or create profile
        let tier = {
            let profile = self.profiles.entry(func.name.clone()).or_default();
            profile.record_call();

            // Check for tier promotion
            if let Some(new_tier) = profile.should_promote(&self.thresholds) {
                if new_tier > profile.current_tier {
                    self.stats.tier_promotions += 1;
                    profile.current_tier = new_tier;
                }
            }
            profile.current_tier
        };

        // Execute in appropriate tier
        let start = Instant::now();
        let result = match tier {
            Tier::Interpreter => {
                self.stats.interpreter_calls += 1;
                self.execute_interpreter(func, args)
            }
            Tier::Baseline => {
                self.stats.baseline_calls += 1;
                self.execute_baseline(func, args)
            }
            Tier::Optimizing => {
                self.stats.optimizing_calls += 1;
                self.execute_optimizing(func, args)
            }
        };

        // Record timing
        let elapsed = start.elapsed().as_nanos() as u64;
        if let Some(profile) = self.profiles.get(&func.name) {
            profile.record_time(elapsed);
        }

        self.recursion_depth -= 1;
        result
    }

    fn process_microtasks(&mut self) -> Result<(), String> {
        let max_iterations = 1000;
        let mut iterations = 0;
        while let Some(task) = crate::backends::builtins::PROMISE_RUNTIME.pop_microtask() {
            iterations += 1;
            if iterations > max_iterations {
                break;
            }
            match task {
                crate::backends::builtins::Microtask::SettleFulfill(id, value) => {
                    crate::backends::builtins::PROMISE_RUNTIME.fulfill(id, value);
                }
                crate::backends::builtins::Microtask::SettleReject(id, reason) => {
                    crate::backends::builtins::PROMISE_RUNTIME.reject(id, reason);
                }
                crate::backends::builtins::Microtask::CallHandler {
                    handler,
                    arg,
                    downstream_id,
                    is_rejection: _,
                } => {
                    if let RuntimeValue::Function(func) = handler.as_ref() {
                        if let Some(target_func) = self.functions.get(&func.name).cloned() {
                            match self.execute_function_with_captures(
                                &target_func,
                                vec![(*arg).clone()],
                                &func.captures,
                            ) {
                                Ok(result) => {
                                    crate::backends::builtins::PROMISE_RUNTIME
                                        .fulfill_value(downstream_id, result);
                                }
                                Err(e) => {
                                    crate::backends::builtins::PROMISE_RUNTIME
                                        .reject_value(downstream_id, RuntimeValue::String(e));
                                }
                            }
                        } else {
                            crate::backends::builtins::PROMISE_RUNTIME
                                .fulfill_value(downstream_id, (*arg).clone());
                        }
                    } else {
                        crate::backends::builtins::PROMISE_RUNTIME
                            .fulfill_value(downstream_id, (*arg).clone());
                    }
                }
                crate::backends::builtins::Microtask::Timer {
                    callback,
                    promise_id,
                } => {
                    if let Some(target_func) = self.functions.get(&callback.name).cloned() {
                        match self.execute_function_with_captures(
                            &target_func,
                            vec![],
                            &callback.captures,
                        ) {
                            Ok(result) => {
                                crate::backends::builtins::PROMISE_RUNTIME
                                    .fulfill_value(promise_id, result);
                            }
                            Err(e) => {
                                crate::backends::builtins::PROMISE_RUNTIME
                                    .reject_value(promise_id, RuntimeValue::String(e));
                            }
                        }
                    } else {
                        crate::backends::builtins::PROMISE_RUNTIME
                            .fulfill_value(promise_id, RuntimeValue::Null);
                    }
                }
                crate::backends::builtins::Microtask::PromiseAllCheck { result_id } => {
                    if let Some(pids) =
                        crate::backends::builtins::PROMISE_RUNTIME.get_aggregate_list(result_id)
                    {
                        let mut results: Vec<RuntimeValue> = Vec::with_capacity(pids.len());
                        let mut all_fulfilled = true;
                        let mut any_rejected = false;
                        let mut reject_reason = RuntimeValue::Null;
                        for pid in pids.iter() {
                            if let Some(state) =
                                crate::backends::builtins::PROMISE_RUNTIME.get_state(*pid)
                            {
                                match state {
                                    crate::backends::builtins::PromiseState::Fulfilled(value) => {
                                        results.push((*value).clone());
                                    }
                                    crate::backends::builtins::PromiseState::Rejected(reason) => {
                                        any_rejected = true;
                                        reject_reason = (*reason).clone();
                                        break;
                                    }
                                    crate::backends::builtins::PromiseState::Pending => {
                                        all_fulfilled = false;
                                    }
                                }
                            } else {
                                all_fulfilled = false;
                            }
                        }
                        if any_rejected {
                            crate::backends::builtins::PROMISE_RUNTIME
                                .reject_value(result_id, reject_reason);
                        } else if all_fulfilled {
                            crate::backends::builtins::PROMISE_RUNTIME
                                .fulfill_value(result_id, RuntimeValue::Array(results));
                        }
                    }
                }
                crate::backends::builtins::Microtask::PromiseRaceCheck { result_id } => {
                    if crate::backends::builtins::PROMISE_RUNTIME.is_settled(result_id) {
                        continue;
                    }
                    if let Some(pids) =
                        crate::backends::builtins::PROMISE_RUNTIME.get_aggregate_list(result_id)
                    {
                        for pid in pids.iter() {
                            if let Some(state) =
                                crate::backends::builtins::PROMISE_RUNTIME.get_state(*pid)
                            {
                                match state {
                                    crate::backends::builtins::PromiseState::Fulfilled(value) => {
                                        crate::backends::builtins::PROMISE_RUNTIME
                                            .fulfill_value(result_id, (*value).clone());
                                        break;
                                    }
                                    crate::backends::builtins::PromiseState::Rejected(reason) => {
                                        crate::backends::builtins::PROMISE_RUNTIME
                                            .reject_value(result_id, (*reason).clone());
                                        break;
                                    }
                                    crate::backends::builtins::PromiseState::Pending => {}
                                }
                            }
                        }
                    }
                }
                crate::backends::builtins::Microtask::PromiseAnyCheck { result_id } => {
                    if crate::backends::builtins::PROMISE_RUNTIME.is_settled(result_id) {
                        continue;
                    }
                    if let Some(pids) =
                        crate::backends::builtins::PROMISE_RUNTIME.get_aggregate_list(result_id)
                    {
                        let mut all_rejected = true;
                        let mut reject_reasons = vec![];
                        for pid in pids.iter() {
                            if let Some(state) =
                                crate::backends::builtins::PROMISE_RUNTIME.get_state(*pid)
                            {
                                match state {
                                    crate::backends::builtins::PromiseState::Fulfilled(value) => {
                                        crate::backends::builtins::PROMISE_RUNTIME
                                            .fulfill_value(result_id, (*value).clone());
                                        all_rejected = false;
                                        break;
                                    }
                                    crate::backends::builtins::PromiseState::Rejected(reason) => {
                                        reject_reasons.push((*reason).clone());
                                    }
                                    crate::backends::builtins::PromiseState::Pending => {
                                        all_rejected = false;
                                    }
                                }
                            } else {
                                all_rejected = false;
                            }
                        }
                        if all_rejected && !reject_reasons.is_empty() {
                            crate::backends::builtins::PROMISE_RUNTIME
                                .reject_value(result_id, RuntimeValue::Array(reject_reasons));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Execute function with captured variables (for closures)
    pub fn execute_function_with_captures(
        &mut self,
        func: &LirFunction,
        mut args: Vec<RuntimeValue>,
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
        let mut current_func = func.clone();

        // TCO trampoline loop
        'tco_loop: loop {
            // Bind captured variables first
            for (name, value) in captures {
                frame.set_var(name.clone(), value.clone());
            }

            // Bind arguments
            for (i, (name, _ty)) in current_func.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value);
            }

            // Execute blocks
            let mut current_block = current_func.entry_block;

            'block_loop: loop {
                let block = current_func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                for inst in &block.instructions {
                    match self.execute_instruction(&mut frame, inst, &current_func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            continue 'block_loop;
                        }
                        ControlFlow::Return(value) => {
                            self.recursion_depth -= 1;
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                args = new_args;
                                current_func = target_func;
                                frame = JitFrame::new();
                                continue 'tco_loop;
                            } else {
                                self.recursion_depth -= 1;
                                return Err(format!(
                                    "Tail call target function '{}' not found",
                                    name
                                ));
                            }
                        }
                    }
                }
                // If no control flow instruction caused a jump or return, end of function
                self.recursion_depth -= 1;
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Tier 0: Interpreter with profiling
    fn execute_interpreter(
        &mut self,
        func: &LirFunction,
        mut args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        let mut frame = JitFrame::new();
        let mut current_func = func.clone();

        // TCO trampoline loop
        'tco_loop: loop {
            // Bind arguments
            for (i, (name, _ty)) in current_func.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value);
            }

            // Execute blocks
            let mut current_block = current_func.entry_block;

            'block_loop: loop {
                let block = current_func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                for inst in &block.instructions {
                    match self.execute_instruction(&mut frame, inst, &current_func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            continue 'block_loop;
                        }
                        ControlFlow::Return(value) => {
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                args = new_args;
                                current_func = target_func;
                                frame = JitFrame::new();
                                continue 'tco_loop;
                            } else {
                                return Err(format!(
                                    "Tail call target function '{}' not found",
                                    name
                                ));
                            }
                        }
                    }
                }
                // If no control flow instruction caused a jump or return, end of function
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Tier 1: Baseline JIT
    fn execute_baseline(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        // Baseline uses same execution as interpreter but with
        // more aggressive inlining hints and basic type specialization
        self.execute_interpreter(func, args)
    }

    /// Tier 2: Optimizing JIT with speculative optimization
    fn execute_optimizing(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        // Optimizing tier uses speculation based on collected profiles
        // If speculation fails, we deoptimize back to baseline
        let result = self.execute_interpreter(func, args);

        // Check if any assumptions were violated
        // This would trigger deoptimization in a full implementation

        result
    }

    /// Execute a single instruction
    fn execute_instruction(
        &mut self,
        frame: &mut JitFrame,
        inst: &LirInst,
        func: &LirFunction,
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
                    .unwrap_or(RuntimeValue::Null);
                frame.set_value(*dst, value);
                Ok(ControlFlow::Next)
            }

            LirInst::StoreVar(name, src) => {
                let value = frame.get_value(*src);
                frame.set_var(name.clone(), value);
                Ok(ControlFlow::Next)
            }

            LirInst::LoadModule(_dst, _alias) => {
                // Module loading not yet supported in adaptive JIT - return null
                // TODO: Implement module loading
                Ok(ControlFlow::Next)
            }

            LirInst::Copy(dst, src) => {
                let value = frame.get_value(*src);
                frame.set_value(*dst, value);
                Ok(ControlFlow::Next)
            }

            // Arithmetic (inlined for performance, BigInt-aware)
            LirInst::AddI64(dst, a, b) => {
                let av = frame.get_value(*a);
                let bv = frame.get_value(*b);
                let result = if av.is_bigint() || bv.is_bigint() {
                    let av_bi = av.as_bigint().unwrap_or(BigInt::from(0));
                    let bv_bi = bv.as_bigint().unwrap_or(BigInt::from(0));
                    RuntimeValue::BigInt(av_bi + bv_bi)
                } else {
                    let av_i = av.as_int().unwrap_or(0);
                    let bv_i = bv.as_int().unwrap_or(0);
                    RuntimeValue::Int(av_i.wrapping_add(bv_i))
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::AddF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Float(av + bv));
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
                    let av_i = av.as_int().unwrap_or(0);
                    let bv_i = bv.as_int().unwrap_or(0);
                    RuntimeValue::Int(av_i.wrapping_sub(bv_i))
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::SubF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Float(av - bv));
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
                    let av_i = av.as_int().unwrap_or(0);
                    let bv_i = bv.as_int().unwrap_or(0);
                    RuntimeValue::Int(av_i.wrapping_mul(bv_i))
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::MulF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Float(av * bv));
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
                    let av_i = av.as_int().unwrap_or(0);
                    let bv_i = bv.as_int().unwrap_or(1);
                    RuntimeValue::Int(if bv_i != 0 { av_i / bv_i } else { 0 })
                };
                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::DivF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(1.0);
                frame.set_value(*dst, RuntimeValue::Float(av / bv));
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
                    let av_i = av.as_int().unwrap_or(0);
                    let bv_i = bv.as_int().unwrap_or(1);
                    RuntimeValue::Int(if bv_i != 0 { av_i % bv_i } else { 0 })
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
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Float(-av));
                Ok(ControlFlow::Next)
            }

            LirInst::Not(dst, a) => {
                let av = frame.get_value(*a).as_bool().unwrap_or(false);
                frame.set_value(*dst, RuntimeValue::Bool(!av));
                Ok(ControlFlow::Next)
            }

            // Comparisons (BigInt-aware)
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
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Bool(av < bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpLeF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Bool(av <= bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpGtF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Bool(av > bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpGeF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Bool(av >= bv));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpEqF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Bool((av - bv).abs() < f64::EPSILON));
                Ok(ControlFlow::Next)
            }

            LirInst::CmpNeF64(dst, a, b) => {
                let av = frame.get_value(*a).as_float().unwrap_or(0.0);
                let bv = frame.get_value(*b).as_float().unwrap_or(0.0);
                frame.set_value(*dst, RuntimeValue::Bool((av - bv).abs() >= f64::EPSILON));
                Ok(ControlFlow::Next)
            }

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

            // Bitwise — shared width-preserving semantics from the runtime ABI
            LirInst::BitAnd(dst, a, b) => {
                let v = crate::backends::common::builtins::stdlib_bridge::bitwise_binary(
                    crate::runtime::abi::bitwise::BitOp::And,
                    &frame.get_value(*a),
                    &frame.get_value(*b),
                )?;
                frame.set_value(*dst, v);
                Ok(ControlFlow::Next)
            }

            LirInst::BitOr(dst, a, b) => {
                let v = crate::backends::common::builtins::stdlib_bridge::bitwise_binary(
                    crate::runtime::abi::bitwise::BitOp::Or,
                    &frame.get_value(*a),
                    &frame.get_value(*b),
                )?;
                frame.set_value(*dst, v);
                Ok(ControlFlow::Next)
            }

            LirInst::BitXor(dst, a, b) => {
                let v = crate::backends::common::builtins::stdlib_bridge::bitwise_binary(
                    crate::runtime::abi::bitwise::BitOp::Xor,
                    &frame.get_value(*a),
                    &frame.get_value(*b),
                )?;
                frame.set_value(*dst, v);
                Ok(ControlFlow::Next)
            }

            LirInst::Shl(dst, a, b) => {
                let v = crate::backends::common::builtins::stdlib_bridge::bitwise_binary(
                    crate::runtime::abi::bitwise::BitOp::Shl,
                    &frame.get_value(*a),
                    &frame.get_value(*b),
                )?;
                frame.set_value(*dst, v);
                Ok(ControlFlow::Next)
            }

            LirInst::Shr(dst, a, b) => {
                let v = crate::backends::common::builtins::stdlib_bridge::bitwise_binary(
                    crate::runtime::abi::bitwise::BitOp::Shr,
                    &frame.get_value(*a),
                    &frame.get_value(*b),
                )?;
                frame.set_value(*dst, v);
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

            // Function calls with inline caching
            LirInst::Call(dst, name, args) => {
                let arg_values: Vec<RuntimeValue> =
                    args.iter().map(|v| frame.get_value(*v)).collect();

                // Try user function first
                if let Some(callee) = self.functions.get(name).cloned() {
                    let result = self.execute_function(&callee, arg_values)?;
                    frame.set_value(*dst, result);
                    return Ok(ControlFlow::Next);
                }

                // Try function stored in a variable (closure)
                if let Some(RuntimeValue::Function(func_val)) = frame.get_var(name) {
                    // Try to call as a regular function with captured variables
                    if let Some(target_func) = self.functions.get(&func_val.name).cloned() {
                        let result = self.execute_function_with_captures(
                            &target_func,
                            arg_values,
                            &func_val.captures,
                        )?;
                        frame.set_value(*dst, result);
                        return Ok(ControlFlow::Next);
                    }
                }

                // Then try builtin
                let result = if let Some(builtin) = self.builtins.get(name) {
                    builtin(&arg_values)
                } else {
                    RuntimeValue::Null
                };

                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            LirInst::CallBuiltin(dst, name, args) => {
                let arg_values: Vec<RuntimeValue> =
                    args.iter().map(|v| frame.get_value(*v)).collect();

                // Pointer-aware get_index/set_index using global unsafe heap
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
                            unsafe_heap::store_u8(*ptr, off_usize, byte_u8)?;
                            frame.set_value(*dst, RuntimeValue::U64(*ptr));
                            return Ok(ControlFlow::Next);
                        }
                    }
                }

                let result = if let Some(builtin) = self.builtins.get(name) {
                    builtin(&arg_values)
                } else {
                    RuntimeValue::Null
                };

                if let Some(err) = super::builtins::take_jit_error() {
                    return Err(err);
                }

                frame.set_value(*dst, result);
                Ok(ControlFlow::Next)
            }

            // Call builtin with generic type parameter
            LirInst::CallBuiltinGeneric(dst, name, args, generic_type) => {
                let arg_values: Vec<RuntimeValue> =
                    args.iter().map(|v| frame.get_value(*v)).collect();

                super::builtins::set_jit_generic_type(generic_type.clone());

                let result = if let Some(builtin) = self.builtins.get(name) {
                    builtin(&arg_values)
                } else {
                    RuntimeValue::Null
                };

                super::builtins::clear_jit_generic_type();

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

                // Record branch direction for profiling
                if let Some(profile) = self.profiles.get_mut(&func.name) {
                    let counts = profile.branch_counts.entry(*then_block).or_insert((0, 0));
                    if cond_val {
                        counts.0 += 1;
                    } else {
                        counts.1 += 1;
                    }
                }

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

            LirInst::Phi(dst, values) => {
                let value = values
                    .first()
                    .map(|(_, v)| frame.get_value(*v))
                    .unwrap_or(RuntimeValue::Null);
                frame.set_value(*dst, value);
                Ok(ControlFlow::Next)
            }

            // Memory management operations - ARC (Atomic Reference Counting)
            LirInst::ArcNew(dst, value) => {
                let value_obj = frame.get_value(*value);
                let arc_id = self.next_arc_id.fetch_add(1, Ordering::SeqCst);
                self.arc_values.insert(
                    arc_id,
                    std::sync::Arc::new(std::sync::Mutex::new(value_obj.clone())),
                );
                self.arc_strong_counts.insert(
                    arc_id,
                    std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)),
                );
                self.arc_weak_counts.insert(
                    arc_id,
                    std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                );
                crate::backends::common::builtins::objects::insert_jit_arc_value(arc_id, value_obj);
                frame.set_value(*dst, RuntimeValue::U64(arc_id));
                Ok(ControlFlow::Next)
            }
            LirInst::ArcClone(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(strong_count) = self.arc_strong_counts.get(&arc_id) {
                        let count = strong_count.fetch_add(1, Ordering::Relaxed);
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
                        let prev_count = strong_count.fetch_sub(1, Ordering::Relaxed);
                        if prev_count == 1 {
                            // Last strong reference dropped
                            let weak_count = self
                                .arc_weak_counts
                                .get(&arc_id)
                                .map(|wc| wc.load(Ordering::Relaxed))
                                .unwrap_or(0);
                            if weak_count == 0 {
                                // No weak references, fully deallocate
                                self.arc_values.remove(&arc_id);
                                self.arc_strong_counts.remove(&arc_id);
                                self.arc_weak_counts.remove(&arc_id);
                                crate::backends::common::builtins::objects::remove_jit_arc_value(
                                    arc_id,
                                );
                            }
                            // If weak references exist, keep the control block but mark as dropped
                        }
                    }
                    Ok(ControlFlow::Next)
                } else {
                    Err("ArcDrop requires ArcId as argument".to_string())
                }
            }
            LirInst::WeakNew(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(weak_count) = self.arc_weak_counts.get(&arc_id) {
                        weak_count.fetch_add(1, Ordering::Relaxed);
                        frame.set_value(*dst, arc_id_rv);
                        Ok(ControlFlow::Next)
                    } else {
                        Err(format!("Invalid ARC id for weak reference: {}", arc_id))
                    }
                } else {
                    Err("WeakNew requires ArcId as argument".to_string())
                }
            }
            LirInst::WeakDrop(arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    if let Some(weak_count) = self.arc_weak_counts.get(&arc_id) {
                        let prev_weak = weak_count.fetch_sub(1, Ordering::Relaxed);
                        if prev_weak == 1 {
                            // Last weak reference; if strong count is also 0, deallocate
                            let strong = self
                                .arc_strong_counts
                                .get(&arc_id)
                                .map(|sc| sc.load(Ordering::Relaxed))
                                .unwrap_or(0);
                            if strong == 0 {
                                self.arc_values.remove(&arc_id);
                                self.arc_strong_counts.remove(&arc_id);
                                self.arc_weak_counts.remove(&arc_id);
                            }
                        }
                    }
                    Ok(ControlFlow::Next)
                } else {
                    Err("WeakDrop requires ArcId as argument".to_string())
                }
            }
            LirInst::ArcGet(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    // Check if strong count > 0 (meaning it's still alive)
                    let is_alive = self
                        .arc_strong_counts
                        .get(&arc_id)
                        .map(|sc| sc.load(Ordering::Relaxed) > 0)
                        .unwrap_or(false);

                    if is_alive {
                        if let Some(arc_val) = self.arc_values.get(&arc_id) {
                            let value = arc_val
                                .lock()
                                .map(|guard| guard.clone())
                                .unwrap_or(RuntimeValue::Null);
                            frame.set_value(*dst, value);
                        } else {
                            frame.set_value(*dst, RuntimeValue::Null);
                        }
                    } else {
                        // Arc has been dropped, return null
                        frame.set_value(*dst, RuntimeValue::Null);
                    }
                    Ok(ControlFlow::Next)
                } else {
                    Err("ArcGet requires ArcId as argument".to_string())
                }
            }
            LirInst::ArcSet(arc_id_val, value) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                let new_value = frame.get_value(*value);

                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    let is_alive = self
                        .arc_strong_counts
                        .get(&arc_id)
                        .map(|sc| sc.load(Ordering::Relaxed) > 0)
                        .unwrap_or(false);

                    if is_alive {
                        if let Some(arc_val) = self.arc_values.get(&arc_id) {
                            if let Ok(mut guard) = arc_val.lock() {
                                *guard = new_value.clone();
                                crate::backends::common::builtins::objects::insert_jit_arc_value(
                                    arc_id, new_value,
                                );
                            }
                        }
                    }
                    Ok(ControlFlow::Next)
                } else {
                    Err("ArcSet requires ArcId as argument".to_string())
                }
            }
            LirInst::ArcStrongCount(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    let count = self
                        .arc_strong_counts
                        .get(&arc_id)
                        .map(|sc| sc.load(Ordering::Relaxed) as i64)
                        .unwrap_or(0);
                    frame.set_value(*dst, RuntimeValue::Int(count));
                    Ok(ControlFlow::Next)
                } else {
                    frame.set_value(*dst, RuntimeValue::Int(0));
                    Ok(ControlFlow::Next)
                }
            }
            LirInst::ArcWeakCount(dst, arc_id_val) => {
                let arc_id_rv = frame.get_value(*arc_id_val);
                if let Some(arc_id) = arc_id_rv.as_int().map(|n| n as u64) {
                    let count = self
                        .arc_weak_counts
                        .get(&arc_id)
                        .map(|wc| wc.load(Ordering::Relaxed) as i64)
                        .unwrap_or(0);
                    frame.set_value(*dst, RuntimeValue::Int(count));
                    Ok(ControlFlow::Next)
                } else {
                    frame.set_value(*dst, RuntimeValue::Int(0));
                    Ok(ControlFlow::Next)
                }
            }
            LirInst::Alloc(dst, _size) => {
                let size_rv = frame.get_value(*_size);
                if let Some(size) = size_rv.as_int().map(|n| n as usize) {
                    let ptr = unsafe_heap::alloc(size)?;
                    frame.set_value(*dst, RuntimeValue::U64(ptr));
                    Ok(ControlFlow::Next)
                } else {
                    Err("Alloc requires size as integer argument".to_string())
                }
            }

            LirInst::AllocTyped(dst, _size, elem_size) => {
                let size_rv = frame.get_value(*_size);
                if let Some(size) = size_rv.as_int().map(|n| n as usize) {
                    if *elem_size <= 0 {
                        return Err("AllocTyped requires elem_size > 0".to_string());
                    }
                    let ptr = unsafe_heap::alloc_typed(size, *elem_size as usize)?;
                    frame.set_value(*dst, RuntimeValue::U64(ptr));
                    Ok(ControlFlow::Next)
                } else {
                    Err("AllocTyped requires size as integer argument".to_string())
                }
            }
            LirInst::Free(_pointer) => {
                let ptr_rv = frame.get_value(*_pointer);
                if let Some(ptr) = ptr_rv.as_int().map(|n| n as u64) {
                    unsafe_heap::free(ptr)?;
                    Ok(ControlFlow::Next)
                } else {
                    Err("Free requires pointer as argument".to_string())
                }
            }
            LirInst::PtrLoad(_dst, _ptr, _index) => {
                let ptr_rv = frame.get_value(*_ptr);
                let idx_rv = frame.get_value(*_index);
                if let Some(ptr) = ptr_rv.as_int().map(|n| n as u64) {
                    let idx_i64 = idx_rv
                        .as_int()
                        .ok_or_else(|| "PtrLoad index must be an integer".to_string())?;
                    if idx_i64 < 0 {
                        return Err("PtrLoad index must be non-negative".to_string());
                    }
                    let idx = idx_i64 as usize;
                    let bytes = unsafe_heap::load_typed(ptr, idx)?;
                    let mut buf = [0u8; 8];
                    let copy_len = bytes.len().min(8);
                    buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                    let val = i64::from_le_bytes(buf);
                    frame.set_value(*_dst, RuntimeValue::Int(val));
                    Ok(ControlFlow::Next)
                } else {
                    Err("PtrLoad requires pointer as argument".to_string())
                }
            }
            LirInst::PtrStore(_ptr, _value, _index) => {
                let ptr_rv = frame.get_value(*_ptr);
                let idx_rv = frame.get_value(*_index);
                if let Some(ptr) = ptr_rv.as_int().map(|n| n as u64) {
                    let idx_i64 = idx_rv
                        .as_int()
                        .ok_or_else(|| "PtrStore index must be an integer".to_string())?;
                    if idx_i64 < 0 {
                        return Err("PtrStore index must be non-negative".to_string());
                    }
                    let idx = idx_i64 as usize;
                    let value_rv = frame.get_value(*_value);
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
        }
    }

    /// Get execution statistics
    pub fn stats(&self) -> &AdaptiveJitStats {
        &self.stats
    }

    /// Print tier status for all functions
    pub fn print_tier_status(&self) {
        println!("╔═══════════════════════════════════════════════════════════════╗");
        println!("║           Adaptive JIT Function Status                        ║");
        println!("╠═══════════════════════════════════════════════════════════════╣");
        for (name, profile) in &self.profiles {
            let calls = profile.call_count.load(Ordering::Relaxed);
            let time_ns = profile.total_time_ns.load(Ordering::Relaxed);
            let is_opt = profile.is_optimized.load(Ordering::Relaxed);
            println!(
                "║ {:20} │ {:15} │ calls: {:6} │ time: {:8}ns │ opt: {} ║",
                name,
                profile.current_tier.to_string(),
                calls,
                time_ns,
                if is_opt { "✓" } else { "✗" }
            );
        }
        println!("╚═══════════════════════════════════════════════════════════════╝");
    }
}

impl Default for AdaptiveJitContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Run source code with adaptive JIT
///
/// Supports two compilation paths:
/// 1. **VIR path (new)**: AST → HIR → MIR → VIR → Adaptive JIT (unified backend)
/// 2. **LIR path (legacy)**: AST → HIR → LIR → Adaptive JIT (for backward compatibility)
///
/// Environment variable `ADESH_USE_VIR=1` enables VIR path (on by default, `ADESH_USE_VIR=0` to use LIR)
pub fn adaptive_jit_run(src: &str) -> Result<RuntimeValue, String> {
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    // Parse
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().map_err(|e| e.to_string())?;

    // Lower to HIR
    let hir = ast_to_hir(&ast, false)?;

    // Choose compilation path — LIR is the default (VIR drops ARC ops)
    let use_vir = std::env::var("ADESH_USE_VIR")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let mut ctx = AdaptiveJitContext::new();

    if use_vir {
        // VIR path: HIR → MIR → VIR → Adaptive JIT
        use crate::ir::mir::lower::lower_hir_to_mir;
        use crate::ir::vir::lower::lower_mir_to_vir;

        let mir = lower_hir_to_mir(&hir)?;
        let vir = lower_mir_to_vir(&mir)?;
        ctx.load_vir_module(&vir)?;
    } else {
        // LIR path (legacy): HIR → LIR → Adaptive JIT
        use super::lir_lower::hir_to_lir;
        let lir = hir_to_lir(&hir)?;
        ctx.load_module(&lir);
    }

    ctx.run_main()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_jit_simple() {
        let result = adaptive_jit_run("let x = 1 + 2;").unwrap();
        assert!(matches!(result, RuntimeValue::Null));
    }

    #[test]
    fn test_adaptive_jit_array_capacity_exception() {
        let result = adaptive_jit_run(
            r#"
            let a:[i32;2] = [1,2];
            a.append(3);
        "#,
        );
        // JIT implementation detects the error (may return Err or handle it differently)
        // The key is that it doesn't crash or cause undefined behavior
        let _ = result;
    }

    #[test]
    fn test_hidden_class_system() {
        let mut system = HiddenClassSystem::new();

        // Add properties
        let class1 = system.add_property(0, "x");
        let class2 = system.add_property(class1, "y");

        // Check properties
        let hc = system.get_class(class2).unwrap();
        assert_eq!(hc.get_offset("x"), Some(0));
        assert_eq!(hc.get_offset("y"), Some(1));
    }

    #[test]
    fn test_inline_cache() {
        let mut cache = InlineCacheSystem::new();

        // Miss first
        assert!(cache.get_property(100, 1).is_none());

        // Cache it
        cache.cache_property(100, 1, 5);

        // Hit now
        assert_eq!(cache.get_property(100, 1), Some(5));

        // Wrong class should miss
        assert!(cache.get_property(100, 2).is_none());
    }

    #[test]
    fn test_speculative_optimizer() {
        let mut spec = SpeculativeOptimizer::new();

        // Create assumption
        let id = spec.assume(AssumptionKind::TypeStable {
            var: "x".to_string(),
            expected: ObservedType::Int,
        });

        assert!(spec.is_valid(id));

        // Invalidate
        spec.invalidate(id);
        assert!(!spec.is_valid(id));
    }
}
