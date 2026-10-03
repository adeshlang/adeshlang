//! Production Compiler Driver, Diagnostics, Statistics, and Caching Framework.
//!
//! Provides the top-level orchestration for native code generation, linking,
//! debug symbol integration, and compiler resource management.

use crate::error::CodegenError;
use crate::machine_ir::NativeModule;
use crate::opt::{LtoConfig, LtoEngine, OptLevel};
use crate::target_spec::TargetSpec;
use adesh_object::AdobObject;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Compiler Resource Limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerResourceLimits {
    pub max_opt_iterations: usize,
    pub max_function_instructions: usize,
    pub max_cfg_blocks: usize,
    pub max_inline_depth: usize,
    pub max_lto_functions: usize,
}

impl Default for CompilerResourceLimits {
    fn default() -> Self {
        Self {
            max_opt_iterations: 16,
            max_function_instructions: 200_000,
            max_cfg_blocks: 10_000,
            max_inline_depth: 8,
            max_lto_functions: 50_000,
        }
    }
}

/// Comprehensive Compiler Performance and Optimization Statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompilerStats {
    pub functions_compiled: usize,
    pub instructions_generated: usize,
    pub instructions_eliminated: usize,
    pub blocks_eliminated: usize,
    pub moves_coalesced: usize,
    pub constants_folded: usize,
    pub dead_stores_eliminated: usize,
    pub subexpressions_eliminated: usize,
    pub loops_optimized: usize,
    pub bounds_checks_eliminated: usize,
    pub tail_calls_optimized: usize,
    pub inlined_calls: usize,
    pub lto_dead_functions_removed: usize,
    pub string_literals_deduplicated: usize,
    pub compile_time_ms: u64,
}

/// Deterministic Compilation Cache Key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompilationCacheKey {
    pub source_hash: u64,
    pub compiler_version: String,
    pub target_triple: String,
    pub opt_level: OptLevel,
    pub lto_mode: String,
    pub feature_flags: Vec<String>,
}

impl CompilationCacheKey {
    pub fn new(
        source_hash: u64,
        target_triple: impl Into<String>,
        opt_level: OptLevel,
        feature_flags: Vec<String>,
    ) -> Self {
        Self {
            source_hash,
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            target_triple: target_triple.into(),
            opt_level,
            lto_mode: format!("{:?}", opt_level),
            feature_flags,
        }
    }
}

/// Driver Configuration Options.
#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub target_spec: TargetSpec,
    pub opt_level: OptLevel,
    pub enable_lto: bool,
    pub lto_config: LtoConfig,
    pub enable_debug_info: bool,
    pub limits: CompilerResourceLimits,
    pub verbose_diagnostics: bool,
}

impl DriverConfig {
    pub fn new(target_spec: TargetSpec) -> Self {
        Self {
            target_spec,
            opt_level: OptLevel::O2,
            enable_lto: false,
            lto_config: LtoConfig::default(),
            enable_debug_info: false,
            limits: CompilerResourceLimits::default(),
            verbose_diagnostics: false,
        }
    }

    pub fn with_opt_level(mut self, opt_level: OptLevel) -> Self {
        self.opt_level = opt_level;
        self
    }

    pub fn with_lto(mut self, enable_lto: bool) -> Self {
        self.enable_lto = enable_lto;
        self
    }
}

/// Production Native Compiler Driver.
pub struct CompilerDriver {
    pub config: DriverConfig,
    pub stats: CompilerStats,
}

impl CompilerDriver {
    pub fn new(config: DriverConfig) -> Self {
        Self {
            config,
            stats: CompilerStats::default(),
        }
    }

    /// Compile a set of NativeModules, optionally executing Link-Time Optimization (LTO),
    /// and generate a validated ADOB object bundle.
    pub fn compile_and_emit_adob(
        &mut self,
        mut modules: Vec<NativeModule>,
    ) -> Result<Vec<AdobObject>, CodegenError> {
        let start = Instant::now();

        // 1. Perform LTO if multiple modules are provided and LTO is enabled
        if self.config.enable_lto && modules.len() > 1 {
            let mut lto_engine = LtoEngine::new(self.config.lto_config.clone());
            for module in &modules {
                lto_engine.add_module(module);
            }
            let lto_report = lto_engine.optimize_modules(&mut modules)?;
            self.stats.lto_dead_functions_removed += lto_report.dead_functions_removed;
            self.stats.inlined_calls += lto_report.inlined_calls;
        }

        // 2. Create native backend for the target
        let mut backend =
            crate::targets::create_backend(self.config.target_spec.descriptor.clone())?;
        backend.set_opt_level(self.config.opt_level);

        // 3. Lower and emit objects
        let mut objects = Vec::with_capacity(modules.len());
        for module in &modules {
            self.stats.functions_compiled += module.functions.len();
            let obj = backend.emit_object(module)?;
            objects.push(obj);
        }

        self.stats.compile_time_ms = start.elapsed().as_millis() as u64;
        Ok(objects)
    }

    /// Helper to compile a single module directly.
    pub fn compile_single_module(
        &mut self,
        module: &NativeModule,
    ) -> Result<AdobObject, CodegenError> {
        let objs = self.compile_and_emit_adob(vec![module.clone()])?;
        objs.into_iter().next().ok_or_else(|| {
            CodegenError::new(
                self.config.target_spec.descriptor.triple_string(),
                "No object generated for single module",
            )
        })
    }
}
