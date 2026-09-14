//! Decorator Pipeline System
//!
//! This module implements the decorator pipeline transformation system that allows
//! decorators to have multiple phases:
//! - compile: Compile-time AST transformation and validation
//! - runtime: Runtime wrapper/interception  
//! - typecheck: Compile-time type constraints
//! - emit: Backend-specific IR mutation hooks
//!
//! The pipeline builder flattens nested decorator applications into an O(N) execution plan.

use crate::parsing::ast::*;
use rustc_hash::FxHashMap as HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Decorator application on a function
#[derive(Clone, Debug)]
pub struct DecoratorApplication {
    pub decorator_name: String,
    pub args: Vec<Expr>,
}

/// Built decorator pipeline for efficient execution
#[derive(Clone)]
pub struct CompiledPipeline {
    pub fn_id: String,
    pub stages: Vec<PipelineStage>,
    pub pipeline_hash: u64,
    /// Whether this pipeline has compile-time phases
    pub has_compile_phases: bool,
    /// Whether this pipeline has runtime phases
    pub has_runtime_phases: bool,
}

/// A single stage in the compiled pipeline
#[derive(Clone, Debug)]
pub struct PipelineStage {
    pub decorator_name: String,
    pub phase_type: PhaseType,
    pub body: std::sync::Arc<Vec<Stmt>>,
    pub args: Vec<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PhaseType {
    Compile,
    Runtime,
    Typecheck,
    Emit,
}

/// Builds a decorator pipeline from a list of decorator applications and definitions
pub struct PipelineBuilder {
    /// Registry of decorator definitions
    decorators: HashMap<String, DecoratorDef>,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self {
            decorators: HashMap::default(),
        }
    }

    /// Register a decorator definition
    pub fn register_decorator(&mut self, def: DecoratorDef) {
        self.decorators.insert(def.name.clone(), def);
    }

    /// Build a pipeline for a function with decorator applications
    ///
    /// # Arguments
    /// * `fn_name` - Name of the function being decorated
    /// * `applications` - List of decorator applications (in order from top to bottom in source)
    ///
    /// # Returns
    /// A compiled pipeline with stages ordered for execution
    pub fn build_pipeline(
        &self,
        fn_name: &str,
        applications: &[DecoratorApplication],
    ) -> Result<CompiledPipeline, String> {
        let mut stages = Vec::new();
        let mut has_compile_phases = false;
        let mut has_runtime_phases = false;

        // Process decorators in reverse order (nearest to function first for runtime)
        // but collect all compile-time phases first
        let mut compile_stages = Vec::new();
        let mut runtime_stages = Vec::new();

        for app in applications.iter().rev() {
            let def = self
                .decorators
                .get(&app.decorator_name)
                .ok_or_else(|| format!("Decorator '{}' not found", app.decorator_name))?;

            // Extract phases from definition
            for phase in &def.phases {
                match phase {
                    DecoratorPhase::Compile(body) => {
                        compile_stages.push(PipelineStage {
                            decorator_name: app.decorator_name.clone(),
                            phase_type: PhaseType::Compile,
                            body: body.clone(),
                            args: app.args.clone(),
                        });
                        has_compile_phases = true;
                    }
                    DecoratorPhase::Typecheck(body) => {
                        compile_stages.push(PipelineStage {
                            decorator_name: app.decorator_name.clone(),
                            phase_type: PhaseType::Typecheck,
                            body: body.clone(),
                            args: app.args.clone(),
                        });
                        has_compile_phases = true;
                    }
                    DecoratorPhase::Runtime(body) => {
                        runtime_stages.push(PipelineStage {
                            decorator_name: app.decorator_name.clone(),
                            phase_type: PhaseType::Runtime,
                            body: body.clone(),
                            args: app.args.clone(),
                        });
                        has_runtime_phases = true;
                    }
                    DecoratorPhase::Emit(_body) => {
                        // Emit phases run during backend code generation, not at runtime
                        // For now, skip them - they'll be handled by backends
                        // TODO: Implement emit phase execution in JIT/AOT/VM/WASM backends
                        continue;
                    }
                }
            }
        }

        // Compile phases run first, then runtime phases
        stages.extend(compile_stages);
        stages.extend(runtime_stages);

        // Calculate pipeline hash for caching
        let hash = calculate_pipeline_hash(fn_name, &stages);

        Ok(CompiledPipeline {
            fn_id: fn_name.to_string(),
            stages,
            pipeline_hash: hash,
            has_compile_phases,
            has_runtime_phases,
        })
    }

    /// Optimize the pipeline by fusing compatible stages
    /// Combines decorators that can be merged to reduce overhead
    pub fn optimize_pipeline(&self, mut pipeline: CompiledPipeline) -> CompiledPipeline {
        // Fusion optimization: Combine consecutive runtime stages that are compatible
        // Compatible stages are those that:
        // 1. Are both Runtime phases
        // 2. Don't have conflicting side effects
        // 3. Can be reordered without changing behavior

        if pipeline.stages.len() <= 1 {
            return pipeline; // Nothing to optimize
        }

        let mut optimized_stages = Vec::new();
        let mut i = 0;

        while i < pipeline.stages.len() {
            let current_stage = &pipeline.stages[i];

            // Check if this stage can be fused with the next one
            if i + 1 < pipeline.stages.len() {
                let next_stage = &pipeline.stages[i + 1];

                // Check if stages can be fused
                if can_fuse_stages(current_stage, next_stage) {
                    // In a full implementation, we would create a merged stage here
                    // For now, since we're conservative and can_fuse_stages returns false,
                    // this branch won't execute. Keep both stages separate.
                    optimized_stages.push(current_stage.clone());
                    i += 1;
                    continue;
                }
            }

            optimized_stages.push(current_stage.clone());
            i += 1;
        }

        pipeline.stages = optimized_stages;
        pipeline
    }
}

/// Check if two stages can be fused together
fn can_fuse_stages(stage1: &PipelineStage, stage2: &PipelineStage) -> bool {
    // Only fuse runtime stages for now
    if stage1.phase_type != PhaseType::Runtime || stage2.phase_type != PhaseType::Runtime {
        return false;
    }

    // Future enhancement: detect compatible decorators
    // For now, conservative approach: don't fuse unless we can prove safety
    // Examples of future fusion opportunities:
    // - Multiple logging/tracing decorators
    // - Multiple validation decorators
    // - Decorators with no side effects that can be reordered

    // Return false for now (conservative)
    // When implementing fusion, check for:
    // 1. No conflicting side effects
    // 2. Compatible execution order
    // 3. Similar decorator semantics (e.g., both are logging)
    false
}

/// Calculate a hash for the pipeline for caching compiled versions
fn calculate_pipeline_hash(fn_name: &str, stages: &[PipelineStage]) -> u64 {
    let mut hasher = DefaultHasher::new();
    fn_name.hash(&mut hasher);
    stages.len().hash(&mut hasher);
    for stage in stages {
        stage.decorator_name.hash(&mut hasher);
        // Hash phase type
        match stage.phase_type {
            PhaseType::Compile => "compile".hash(&mut hasher),
            PhaseType::Runtime => "runtime".hash(&mut hasher),
            PhaseType::Typecheck => "typecheck".hash(&mut hasher),
            PhaseType::Emit => "emit".hash(&mut hasher),
        }
    }
    hasher.finish()
}

/// Maximum number of decorators allowed to prevent infinite loops
/// Set to a high value to support complex decorator chains while catching pathological cases
const MAX_DECORATOR_DEPTH: usize = 10000;

/// Validate that decorator applications don't exceed depth limit
/// This is a safety check to prevent infinite recursion or accidental misuse
pub fn validate_decorator_depth(count: usize) -> Result<(), String> {
    if count > MAX_DECORATOR_DEPTH {
        Err(format!(
            "Too many decorators ({}). Maximum allowed is {} (this is a safety limit to prevent infinite recursion)",
            count, MAX_DECORATOR_DEPTH
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_pipeline_builder_creation() {
        let builder = PipelineBuilder::new();
        assert_eq!(builder.decorators.len(), 0);
    }

    #[test]
    fn test_decorator_registration() {
        let mut builder = PipelineBuilder::new();
        let def = DecoratorDef {
            name: "test".to_string(),
            params: vec![],
            phases: vec![],
            requires_unsafe: false,
            is_new_style: true,
        };
        builder.register_decorator(def);
        assert_eq!(builder.decorators.len(), 1);
    }

    #[test]
    fn test_validate_decorator_depth() {
        assert!(validate_decorator_depth(10).is_ok());
        assert!(validate_decorator_depth(50).is_ok());
        assert!(validate_decorator_depth(100).is_ok());
        assert!(validate_decorator_depth(1000).is_ok());
        assert!(validate_decorator_depth(5000).is_ok());
        assert!(validate_decorator_depth(10000).is_ok());
        assert!(validate_decorator_depth(10001).is_err());
    }

    #[test]
    fn test_fusion_optimizer() {
        let builder = PipelineBuilder::new();

        // Create a simple pipeline
        let pipeline = CompiledPipeline {
            fn_id: "test_fn".to_string(),
            stages: vec![
                PipelineStage {
                    decorator_name: "log".to_string(),
                    phase_type: PhaseType::Runtime,
                    body: Arc::new(vec![]),
                    args: vec![],
                },
                PipelineStage {
                    decorator_name: "trace".to_string(),
                    phase_type: PhaseType::Runtime,
                    body: Arc::new(vec![]),
                    args: vec![],
                },
            ],
            pipeline_hash: 0,
            has_compile_phases: false,
            has_runtime_phases: true,
        };

        let optimized = builder.optimize_pipeline(pipeline.clone());
        // Currently conservative, so should return same stages
        assert_eq!(optimized.stages.len(), pipeline.stages.len());
    }

    #[test]
    fn test_can_fuse_stages() {
        let stage1 = PipelineStage {
            decorator_name: "log".to_string(),
            phase_type: PhaseType::Runtime,
            body: Arc::new(vec![]),
            args: vec![],
        };

        let stage2 = PipelineStage {
            decorator_name: "trace".to_string(),
            phase_type: PhaseType::Runtime,
            body: Arc::new(vec![]),
            args: vec![],
        };

        // Currently conservative
        assert!(!can_fuse_stages(&stage1, &stage2));
    }
}
