//! Recursion Optimization Pack for AdeshLang
//!
//! This module provides comprehensive optimizations for recursive functions:
//! - Pure recursion analysis
//! - Tail call optimization (TCO)
//! - Memoization for pure functions
//! - JIT recursion inlining hints
//! - Trampolining for deep recursion

use super::builtins::RuntimeValue;
use super::lir::{BlockId, LirFunction, LirInst, LirModule};
use crate::utils::collections::FastMap;

// ============================================================================
// Pure Recursion Analyzer
// ============================================================================

/// Classification of a function's purity and recursion pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursionKind {
    /// Function is not recursive
    NonRecursive,
    /// Function is pure and recursive (safe for memoization)
    PureRecursive,
    /// Function is recursive but has side effects
    ImpureRecursive,
    /// Function is tail-recursive (eligible for TCO)
    TailRecursive,
    /// Function has mutual recursion with other functions
    MutualRecursive,
}

/// Analysis result for a function
#[derive(Debug, Clone)]
pub struct RecursionAnalysis {
    pub kind: RecursionKind,
    pub is_pure: bool,
    pub has_tail_calls: bool,
    pub recursive_call_sites: Vec<BlockId>,
    pub call_depth_estimate: Option<u32>,
    /// Set of functions this function calls
    pub callees: Vec<String>,
}

impl Default for RecursionAnalysis {
    fn default() -> Self {
        RecursionAnalysis {
            kind: RecursionKind::NonRecursive,
            is_pure: true,
            has_tail_calls: false,
            recursive_call_sites: Vec::new(),
            call_depth_estimate: None,
            callees: Vec::new(),
        }
    }
}

/// Analyzer for detecting recursion patterns and purity
pub struct PureRecursionAnalyzer {
    /// Cached analysis results
    results: FastMap<String, RecursionAnalysis>,
    /// Functions known to have side effects
    impure_builtins: Vec<&'static str>,
}

impl PureRecursionAnalyzer {
    pub fn new() -> Self {
        PureRecursionAnalyzer {
            results: FastMap::default(),
            impure_builtins: vec![
                "print",
                "println",
                "input",
                "write",
                "read",
                "Date.now",
                "Math.random",
                "clock",
            ],
        }
    }

    /// Analyze an entire module
    pub fn analyze_module(&mut self, module: &LirModule) {
        // First pass: identify all functions and their callees
        for func in &module.functions {
            let analysis = self.analyze_function(func);
            self.results.insert(func.name.clone(), analysis);
        }

        // Second pass: detect mutual recursion
        self.detect_mutual_recursion(module);
    }

    /// Analyze a single function
    pub fn analyze_function(&self, func: &LirFunction) -> RecursionAnalysis {
        let mut analysis = RecursionAnalysis::default();
        let mut is_pure = true;
        let mut has_self_call = false;
        let mut tail_call_blocks = Vec::new();

        for block in &func.blocks {
            let mut last_is_return = false;
            let mut last_call_to_self = false;

            for inst in &block.instructions {
                match inst {
                    LirInst::Call(_, name, _) => {
                        analysis.callees.push(name.clone());
                        if name == &func.name {
                            has_self_call = true;
                            last_call_to_self = true;
                            analysis.recursive_call_sites.push(block.id);
                        } else {
                            last_call_to_self = false;
                        }
                    }
                    LirInst::CallBuiltin(_, name, _) => {
                        analysis.callees.push(name.clone());
                        // Check if this builtin has side effects
                        if self.impure_builtins.iter().any(|&b| name.contains(b)) {
                            is_pure = false;
                        }
                        last_call_to_self = false;
                    }
                    LirInst::Return(_) => {
                        last_is_return = true;
                    }
                    _ => {
                        last_call_to_self = false;
                    }
                }
            }

            // Check for tail call: last instruction before return is self-call
            if last_is_return && last_call_to_self {
                tail_call_blocks.push(block.id);
            }
        }

        analysis.is_pure = is_pure;

        // Determine recursion kind
        if !has_self_call {
            analysis.kind = RecursionKind::NonRecursive;
        } else if !tail_call_blocks.is_empty()
            && tail_call_blocks.len() == analysis.recursive_call_sites.len()
        {
            analysis.kind = RecursionKind::TailRecursive;
            analysis.has_tail_calls = true;
        } else if is_pure {
            analysis.kind = RecursionKind::PureRecursive;
        } else {
            analysis.kind = RecursionKind::ImpureRecursive;
        }

        analysis
    }

    /// Detect mutual recursion between functions
    fn detect_mutual_recursion(&mut self, module: &LirModule) {
        // Build call graph
        let mut call_graph: FastMap<String, Vec<String>> = FastMap::default();
        for func in &module.functions {
            if let Some(analysis) = self.results.get(&func.name) {
                call_graph.insert(func.name.clone(), analysis.callees.clone());
            }
        }

        // Check for cycles (simplified - just check A -> B -> A)
        for (func_name, callees) in &call_graph {
            for callee in callees {
                if let Some(callee_callees) = call_graph.get(callee) {
                    if callee_callees.contains(func_name) && func_name != callee {
                        // Mutual recursion detected
                        if let Some(analysis) = self.results.get_mut(func_name) {
                            analysis.kind = RecursionKind::MutualRecursive;
                        }
                    }
                }
            }
        }
    }

    /// Get analysis for a function
    pub fn get_analysis(&self, name: &str) -> Option<&RecursionAnalysis> {
        self.results.get(name)
    }
}

impl Default for PureRecursionAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tail Call Optimization
// ============================================================================

/// Tail call optimizer that transforms eligible recursive calls into loops
pub struct TailCallOptimizer {
    enabled: bool,
}

impl TailCallOptimizer {
    pub fn new(enabled: bool) -> Self {
        TailCallOptimizer { enabled }
    }

    /// Check if a function is eligible for TCO
    pub fn is_eligible(&self, analysis: &RecursionAnalysis) -> bool {
        self.enabled && matches!(analysis.kind, RecursionKind::TailRecursive)
    }

    /// Transform a tail-recursive function - convert tail calls to TailCall instructions
    /// Returns the transformed function or None if not eligible
    pub fn optimize(
        &self,
        func: &LirFunction,
        analysis: &RecursionAnalysis,
    ) -> Option<LirFunction> {
        if !self.is_eligible(analysis) {
            return None;
        }

        // Clone and transform the function
        let mut optimized = func.clone();

        // Transform each block: look for Call to self followed by Return of that value
        for block in &mut optimized.blocks {
            let mut new_instructions = Vec::new();
            let mut skip_next_return = false;

            let mut i = 0;
            while i < block.instructions.len() {
                let inst = &block.instructions[i];

                if skip_next_return {
                    // We converted the previous call to TailCall, skip this return
                    skip_next_return = false;
                    i += 1;
                    continue;
                }

                // Check if this is a Call to self followed by Return of the call result
                if let LirInst::Call(dst, name, args) = inst {
                    if name == &func.name && i + 1 < block.instructions.len() {
                        if let LirInst::Return(Some(ret_val)) = &block.instructions[i + 1] {
                            if ret_val == dst {
                                // This is a tail call! Convert it
                                new_instructions
                                    .push(LirInst::TailCall(name.clone(), args.clone()));
                                skip_next_return = true;
                                i += 1;
                                continue;
                            }
                        }
                    }
                }

                new_instructions.push(inst.clone());
                i += 1;
            }

            block.instructions = new_instructions;
        }

        Some(optimized)
    }
}

impl Default for TailCallOptimizer {
    fn default() -> Self {
        Self::new(true)
    }
}

// ============================================================================
// Memoization Engine
// ============================================================================

/// Memoization cache for pure recursive functions
pub struct MemoCache {
    /// Cache for single-argument functions (key: i64)
    int_cache: FastMap<i64, RuntimeValue>,
    /// Cache for multi-argument functions (key: tuple of args as string)
    multi_cache: FastMap<String, RuntimeValue>,
    /// Maximum cache size
    max_size: usize,
    /// Hit count for statistics
    hits: u64,
    /// Miss count for statistics
    misses: u64,
}

impl MemoCache {
    pub fn new(max_size: usize) -> Self {
        MemoCache {
            int_cache: FastMap::default(),
            multi_cache: FastMap::default(),
            max_size,
            hits: 0,
            misses: 0,
        }
    }

    /// Look up a single integer argument
    pub fn get_int(&mut self, key: i64) -> Option<RuntimeValue> {
        if let Some(val) = self.int_cache.get(&key) {
            self.hits += 1;
            Some(val.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Store a single integer argument result
    pub fn set_int(&mut self, key: i64, value: RuntimeValue) {
        if self.int_cache.len() < self.max_size {
            self.int_cache.insert(key, value);
        }
    }

    /// Look up multiple arguments
    pub fn get_multi(&mut self, args: &[RuntimeValue]) -> Option<RuntimeValue> {
        let key = args_to_key(args);
        if let Some(val) = self.multi_cache.get(&key) {
            self.hits += 1;
            Some(val.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Store multiple arguments result
    pub fn set_multi(&mut self, args: &[RuntimeValue], value: RuntimeValue) {
        if self.multi_cache.len() < self.max_size {
            let key = args_to_key(args);
            self.multi_cache.insert(key, value);
        }
    }

    /// Clear all caches
    pub fn clear(&mut self) {
        self.int_cache.clear();
        self.multi_cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> (u64, u64, f64) {
        let total = self.hits + self.misses;
        let hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
        (self.hits, self.misses, hit_rate)
    }
}

impl Default for MemoCache {
    fn default() -> Self {
        Self::new(10000)
    }
}

/// Convert arguments to a cache key string
fn args_to_key(args: &[RuntimeValue]) -> String {
    args.iter()
        .map(|v| match v {
            RuntimeValue::Int(n) => format!("i{}", n),
            RuntimeValue::Float(n) => format!("f{:.10}", n),
            RuntimeValue::Bool(b) => format!("b{}", b),
            RuntimeValue::String(s) => format!("s{}", s),
            RuntimeValue::BigInt(bi) => format!("bi{}", bi),
            RuntimeValue::Null => "null".to_string(),
            // Handle fixed-width unsigned integers
            RuntimeValue::U8(n) => format!("u8:{}", n),
            RuntimeValue::U16(n) => format!("u16:{}", n),
            RuntimeValue::U32(n) => format!("u32:{}", n),
            RuntimeValue::U64(n) => format!("u64:{}", n),
            RuntimeValue::U128(n) => format!("u128:{}", n),
            // Handle fixed-width signed integers
            RuntimeValue::I8(n) => format!("i8:{}", n),
            RuntimeValue::I16(n) => format!("i16:{}", n),
            RuntimeValue::I32(n) => format!("i32:{}", n),
            RuntimeValue::I64(n) => format!("i64:{}", n),
            RuntimeValue::I128(n) => format!("i128:{}", n),
            // Handle fixed-width floats
            RuntimeValue::F32(n) => format!("f32:{:.10}", n),
            RuntimeValue::F64(n) => format!("f64:{:.10}", n),
            // For complex types, use their as_int or as_string representation
            _ => {
                // Try to convert to int for numeric types, otherwise use string representation
                if let Some(n) = v.as_int() {
                    format!("n{}", n)
                } else {
                    format!("?{}", v.as_string())
                }
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Memoization engine that manages caches for multiple functions
pub struct MemoizationEngine {
    caches: FastMap<String, MemoCache>,
    enabled: bool,
    /// Functions eligible for memoization
    eligible_functions: FastMap<String, bool>,
}

impl MemoizationEngine {
    pub fn new(enabled: bool) -> Self {
        MemoizationEngine {
            caches: FastMap::default(),
            enabled,
            eligible_functions: FastMap::default(),
        }
    }

    /// Register a function as eligible for memoization
    pub fn register(&mut self, func_name: &str, analysis: &RecursionAnalysis) {
        if self.enabled
            && analysis.is_pure
            && matches!(
                analysis.kind,
                RecursionKind::PureRecursive | RecursionKind::TailRecursive
            )
        {
            self.eligible_functions.insert(func_name.to_string(), true);
            self.caches
                .insert(func_name.to_string(), MemoCache::default());
        }
    }

    /// Check if function is memoized
    pub fn is_memoized(&self, func_name: &str) -> bool {
        self.enabled
            && self
                .eligible_functions
                .get(func_name)
                .copied()
                .unwrap_or(false)
    }

    /// Get cached result for single int arg
    pub fn get_cached_int(&mut self, func_name: &str, arg: i64) -> Option<RuntimeValue> {
        if !self.enabled {
            return None;
        }
        self.caches.get_mut(func_name)?.get_int(arg)
    }

    /// Cache result for single int arg
    pub fn cache_int(&mut self, func_name: &str, arg: i64, result: RuntimeValue) {
        if self.enabled {
            if let Some(cache) = self.caches.get_mut(func_name) {
                cache.set_int(arg, result);
            }
        }
    }

    /// Get cached result for multiple args
    pub fn get_cached_multi(
        &mut self,
        func_name: &str,
        args: &[RuntimeValue],
    ) -> Option<RuntimeValue> {
        if !self.enabled {
            return None;
        }
        self.caches.get_mut(func_name)?.get_multi(args)
    }

    /// Cache result for multiple args
    pub fn cache_multi(&mut self, func_name: &str, args: &[RuntimeValue], result: RuntimeValue) {
        if self.enabled {
            if let Some(cache) = self.caches.get_mut(func_name) {
                cache.set_multi(args, result);
            }
        }
    }

    /// Clear all caches
    pub fn clear_all(&mut self) {
        for cache in self.caches.values_mut() {
            cache.clear();
        }
    }

    /// Get overall statistics
    pub fn stats(&self) -> FastMap<String, (u64, u64, f64)> {
        let mut stats = FastMap::default();
        for (name, cache) in &self.caches {
            stats.insert(name.clone(), cache.stats());
        }
        stats
    }
}

impl Default for MemoizationEngine {
    fn default() -> Self {
        Self::new(true)
    }
}

// ============================================================================
// Trampolining for Deep Recursion
// ============================================================================

/// Trampoline execution state
#[derive(Debug, Clone)]
pub enum TrampolineResult {
    /// Continue with next call
    Continue(String, Vec<RuntimeValue>),
    /// Done with final value
    Done(RuntimeValue),
}

/// Trampoline executor for deep recursion
pub struct TrampolineExecutor {
    enabled: bool,
    max_depth: u32,
    current_depth: u32,
}

impl TrampolineExecutor {
    pub fn new(enabled: bool, max_depth: u32) -> Self {
        TrampolineExecutor {
            enabled,
            max_depth,
            current_depth: 0,
        }
    }

    /// Check if trampolining should be used
    pub fn should_trampoline(&self) -> bool {
        self.enabled && self.current_depth > self.max_depth
    }

    /// Increment depth counter
    pub fn enter(&mut self) {
        self.current_depth += 1;
    }

    /// Decrement depth counter
    pub fn exit(&mut self) {
        if self.current_depth > 0 {
            self.current_depth -= 1;
        }
    }

    /// Reset depth counter
    pub fn reset(&mut self) {
        self.current_depth = 0;
    }

    /// Get current depth
    pub fn depth(&self) -> u32 {
        self.current_depth
    }
}

impl Default for TrampolineExecutor {
    fn default() -> Self {
        Self::new(true, 1000)
    }
}

// ============================================================================
// Recursion Optimization Configuration
// ============================================================================

/// Configuration for recursion optimizations
#[derive(Debug, Clone)]
pub struct RecursionOptConfig {
    /// Enable tail call optimization
    pub tco_enabled: bool,
    /// Enable memoization
    pub memo_enabled: bool,
    /// Enable trampolining
    pub trampoline_enabled: bool,
    /// Enable JIT inlining hints
    pub jit_inline_enabled: bool,
    /// Maximum recursion depth before trampolining
    pub max_recursion_depth: u32,
    /// Maximum memoization cache size
    pub max_cache_size: usize,
}

impl Default for RecursionOptConfig {
    fn default() -> Self {
        RecursionOptConfig {
            tco_enabled: true,
            memo_enabled: true,
            trampoline_enabled: true,
            jit_inline_enabled: true,
            max_recursion_depth: 1000,
            max_cache_size: 10000,
        }
    }
}

impl RecursionOptConfig {
    /// Create config with all optimizations disabled
    pub fn none() -> Self {
        RecursionOptConfig {
            tco_enabled: false,
            memo_enabled: false,
            trampoline_enabled: false,
            jit_inline_enabled: false,
            max_recursion_depth: 1000,
            max_cache_size: 0,
        }
    }

    /// Create config with full optimizations
    pub fn full() -> Self {
        RecursionOptConfig::default()
    }

    /// Create config from string (for CLI parsing)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "full" | "all" => Some(Self::full()),
            "none" | "off" => Some(Self::none()),
            "tco" | "tailcall" => Some(RecursionOptConfig {
                tco_enabled: true,
                memo_enabled: false,
                trampoline_enabled: false,
                jit_inline_enabled: false,
                max_recursion_depth: 1000,
                max_cache_size: 0,
            }),
            "memo" | "memoize" => Some(RecursionOptConfig {
                tco_enabled: false,
                memo_enabled: true,
                trampoline_enabled: false,
                jit_inline_enabled: false,
                max_recursion_depth: 1000,
                max_cache_size: 10000,
            }),
            "jit-inline" | "inline" => Some(RecursionOptConfig {
                tco_enabled: false,
                memo_enabled: false,
                trampoline_enabled: false,
                jit_inline_enabled: true,
                max_recursion_depth: 1000,
                max_cache_size: 0,
            }),
            _ => None,
        }
    }
}

// ============================================================================
// Integrated Recursion Optimizer
// ============================================================================

/// Main recursion optimizer that coordinates all optimization passes
pub struct RecursionOptimizer {
    config: RecursionOptConfig,
    analyzer: PureRecursionAnalyzer,
    tco: TailCallOptimizer,
    memo: MemoizationEngine,
    trampoline: TrampolineExecutor,
}

impl RecursionOptimizer {
    pub fn new(config: RecursionOptConfig) -> Self {
        RecursionOptimizer {
            tco: TailCallOptimizer::new(config.tco_enabled),
            memo: MemoizationEngine::new(config.memo_enabled),
            trampoline: TrampolineExecutor::new(
                config.trampoline_enabled,
                config.max_recursion_depth,
            ),
            analyzer: PureRecursionAnalyzer::new(),
            config,
        }
    }

    /// Analyze and prepare optimizations for a module
    pub fn prepare_module(&mut self, module: &LirModule) {
        // Analyze all functions
        self.analyzer.analyze_module(module);

        // Register eligible functions for memoization
        for func in &module.functions {
            if let Some(analysis) = self.analyzer.get_analysis(&func.name) {
                self.memo.register(&func.name, analysis);
            }
        }
    }

    /// Get the analyzer
    pub fn analyzer(&self) -> &PureRecursionAnalyzer {
        &self.analyzer
    }

    /// Get the memoization engine
    pub fn memo(&mut self) -> &mut MemoizationEngine {
        &mut self.memo
    }

    /// Get the trampoline executor
    pub fn trampoline(&mut self) -> &mut TrampolineExecutor {
        &mut self.trampoline
    }

    /// Get the TCO optimizer
    pub fn tco(&self) -> &TailCallOptimizer {
        &self.tco
    }

    /// Get current configuration
    pub fn config(&self) -> &RecursionOptConfig {
        &self.config
    }

    /// Print optimization summary
    pub fn print_summary(&self) {
        println!("AdeshLang Recursive Optimizer Pack:");
        println!(
            "  TCO:          {}",
            if self.config.tco_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!(
            "  Memoization:  {}",
            if self.config.memo_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!(
            "  Trampolining: {}",
            if self.config.trampoline_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!(
            "  JIT Inlining: {}",
            if self.config.jit_inline_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
    }
}

impl Default for RecursionOptimizer {
    fn default() -> Self {
        Self::new(RecursionOptConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recursion_opt_config_from_str() {
        assert!(RecursionOptConfig::from_str("full").is_some());
        assert!(RecursionOptConfig::from_str("none").is_some());
        assert!(RecursionOptConfig::from_str("tco").is_some());
        assert!(RecursionOptConfig::from_str("memo").is_some());
        assert!(RecursionOptConfig::from_str("invalid").is_none());
    }

    #[test]
    fn test_memo_cache_int() {
        let mut cache = MemoCache::new(100);
        assert!(cache.get_int(5).is_none());
        cache.set_int(5, RuntimeValue::Int(120));
        assert_eq!(cache.get_int(5), Some(RuntimeValue::Int(120)));
    }

    #[test]
    fn test_memo_cache_stats() {
        let mut cache = MemoCache::new(100);
        cache.get_int(1); // miss
        cache.set_int(1, RuntimeValue::Int(1));
        cache.get_int(1); // hit
        cache.get_int(1); // hit
        let (hits, misses, rate) = cache.stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
        assert!((rate - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_trampoline_depth() {
        let mut trampoline = TrampolineExecutor::new(true, 10);
        for _ in 0..15 {
            trampoline.enter();
        }
        assert!(trampoline.should_trampoline());
        trampoline.reset();
        assert!(!trampoline.should_trampoline());
    }

    #[test]
    fn test_recursion_optimizer_creation() {
        let optimizer = RecursionOptimizer::default();
        assert!(optimizer.config().tco_enabled);
        assert!(optimizer.config().memo_enabled);
    }
}
