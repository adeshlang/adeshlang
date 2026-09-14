//! Parallel loop transformation

use super::analysis::IterationIndependence;
use super::cost_model::{ParallelCostModel, ParallelStrategy};

/// A chunk of parallel work
#[derive(Debug, Clone)]
pub struct WorkChunk {
    pub start: u64,
    pub end: u64,
    pub chunk_id: u32,
}

/// Result of parallel loop transformation
#[derive(Debug)]
pub struct ParallelTransformResult {
    pub strategy: ParallelStrategy,
    pub chunks: Vec<WorkChunk>,
    pub safe: bool,
    pub diagnostic: Option<String>,
}

pub struct ParallelTransformer {
    cost_model: ParallelCostModel,
}

impl Default for ParallelTransformer {
    fn default() -> Self {
        ParallelTransformer {
            cost_model: ParallelCostModel::default(),
        }
    }
}

impl ParallelTransformer {
    pub fn transform_loop(
        &self,
        start: u64,
        end: u64,
        independence: &IterationIndependence,
        is_nested: bool,
    ) -> ParallelTransformResult {
        if !independence.is_independent {
            return ParallelTransformResult {
                strategy: ParallelStrategy::Sequential,
                chunks: vec![],
                safe: false,
                diagnostic: Some(
                    "loop not parallelized: iterations are not independent".to_string(),
                ),
            };
        }

        let iterations = end.saturating_sub(start);
        let num_workers = crate::runtime::thread::logical_cpu_count();
        let strategy = self
            .cost_model
            .select_strategy(iterations, 1.0, num_workers, is_nested);

        if strategy == ParallelStrategy::Sequential {
            return ParallelTransformResult {
                strategy,
                chunks: vec![],
                safe: true,
                diagnostic: Some(
                    "loop not parallelized: workload too small for thread overhead".to_string(),
                ),
            };
        }

        let chunk_size = self.cost_model.optimal_chunk_size(iterations, num_workers);
        let mut chunks = Vec::new();
        let mut chunk_id = 0u32;
        let mut pos = start;
        while pos < end {
            let chunk_end = (pos + chunk_size).min(end);
            chunks.push(WorkChunk {
                start: pos,
                end: chunk_end,
                chunk_id,
            });
            chunk_id += 1;
            pos = chunk_end;
        }

        ParallelTransformResult {
            strategy,
            chunks,
            safe: true,
            diagnostic: None,
        }
    }
}
